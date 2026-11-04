//! Acceptance sampling by attributes: operating-characteristic curves.
//!
//! A single-sampling plan takes `n` items from a lot and accepts it when the
//! number of defectives is at most `c`. The **probability of acceptance** of a
//! lot with defective rate `p` is `P(X ≤ c)`:
//!
//! - **large (or infinite) lots**: `X ~ Binomial(n, p)`;
//! - **finite lots**: `X ~ Hypergeometric(N, D, n)` with `D = round(p·N)`,
//!   because sampling without replacement changes the probability.
//!
//! The report draws the **OC curve** `Pa(p)`, gives the acceptance probability
//! at a few reference points and, when asked, **plans a sampling scheme** for a
//! pair `(AQL, LTPD)` with the producer and consumer risks: the smallest sample
//! that gives `Pa(AQL) ≥ 1 − α` and `Pa(LTPD) ≤ β`.
//!
//! The comparison run has acceptance sampling; its lot-size and rounding
//! conventions were not captured, so the numbers are not claimed as reference
//! parity — see `docs/parity.md`.

use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{ln_gamma, regularized_beta};
use crate::workflow::AnalysisOutput;

/// Operating-characteristic curve of a sampling plan.
#[derive(Debug, Clone, PartialEq)]
pub struct OcCurve {
    /// Lot size, when the sampling is without replacement.
    pub lot_size: Option<usize>,
    /// Sample size.
    pub sample: usize,
    /// Acceptance number.
    pub acceptance: usize,
    /// Defective rates of the curve.
    pub proportions: Vec<f64>,
    /// Acceptance probability at each rate.
    pub probabilities: Vec<f64>,
    /// Average outgoing quality limit of the plan.
    pub aoql: Option<(f64, f64)>,
}

/// `P(X ≤ c)` for `X ~ Binomial(n, p)`.
#[must_use]
pub fn binomial_pa(sample: usize, acceptance: usize, p: f64) -> f64 {
    if p <= 0.0 {
        return 1.0;
    }
    if p >= 1.0 {
        return if acceptance >= sample { 1.0 } else { 0.0 };
    }
    if acceptance >= sample {
        return 1.0;
    }
    // I_(1−p)(n − c, c + 1) is P(X ≤ c) for the binomial.
    let n = sample as f64;
    let c = acceptance as f64;
    regularized_beta(1.0 - p, n - c, c + 1.0).clamp(0.0, 1.0)
}

/// `P(X ≤ c)` for `X ~ Hypergeometric(N, D, n)`.
#[must_use]
pub fn hypergeometric_pa(
    lot_size: usize,
    defectives: usize,
    sample: usize,
    acceptance: usize,
) -> f64 {
    if lot_size == 0 || sample == 0 {
        return f64::NAN;
    }
    let sample = sample.min(lot_size);
    let defectives = defectives.min(lot_size);
    let good = lot_size - defectives;
    let log_denominator = ln_gamma(lot_size as f64 + 1.0)
        - ln_gamma(sample as f64 + 1.0)
        - ln_gamma((lot_size - sample) as f64 + 1.0);
    let lowest = sample.saturating_sub(good);
    let highest = acceptance.min(sample).min(defectives);
    if lowest > highest {
        return 0.0;
    }
    let mut sum = 0.0;
    for x in lowest..=highest {
        if x > defectives || sample - x > good {
            continue;
        }
        let log_term = ln_gamma(defectives as f64 + 1.0)
            - ln_gamma(x as f64 + 1.0)
            - ln_gamma((defectives - x) as f64 + 1.0)
            + ln_gamma(good as f64 + 1.0)
            - ln_gamma((sample - x) as f64 + 1.0)
            - ln_gamma((good - (sample - x)) as f64 + 1.0);
        sum += (log_term - log_denominator).exp();
    }
    sum.clamp(0.0, 1.0)
}

/// Acceptance probability of a plan at a defective rate.
fn acceptance_probability(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
    p: f64,
) -> f64 {
    match lot_size {
        Some(lot) => {
            let defectives = (p * lot as f64).round().clamp(0.0, lot as f64) as usize;
            hypergeometric_pa(lot, defectives, sample, acceptance)
        }
        None => binomial_pa(sample, acceptance, p),
    }
}

/// Builds the OC curve of a plan.
pub fn oc_curve(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
    steps: usize,
) -> Result<OcCurve> {
    if sample == 0 {
        return Err(Error::InvalidParameter {
            name: "sample size",
            value: 0.0,
        });
    }
    if acceptance >= sample {
        return Err(Error::Unsupported(
            "the acceptance number must be below the sample size".to_owned(),
        ));
    }
    if let Some(lot) = lot_size
        && (lot == 0 || sample > lot)
    {
        return Err(Error::Unsupported(
            "the sample must not exceed the lot size".to_owned(),
        ));
    }
    let steps = steps.clamp(10, 400);
    let proportions: Vec<f64> = (0..=steps)
        .map(|index| index as f64 / steps as f64 * 0.5)
        .collect();
    let probabilities: Vec<f64> = proportions
        .iter()
        .map(|p| acceptance_probability(lot_size, sample, acceptance, *p))
        .collect();
    // Average outgoing quality: AOQ(p) = p·Pa(p)·(N − n)/N for finite lots and
    // p·Pa(p) otherwise; its maximum over the curve is the AOQL.
    let scale = lot_size.map_or(1.0, |lot| (lot - sample) as f64 / lot as f64);
    let mut aoql: Option<(f64, f64)> = None;
    for (p, pa) in proportions.iter().zip(&probabilities) {
        let aoq = p * pa * scale;
        if aoql.is_none_or(|(_, best)| aoq > best) {
            aoql = Some((*p, aoq));
        }
    }
    Ok(OcCurve {
        lot_size,
        sample,
        acceptance,
        proportions,
        probabilities,
        aoql,
    })
}

/// Sample size and acceptance number that satisfy the two risks.
#[derive(Debug, Clone, PartialEq)]
pub struct SamplingPlan {
    /// Sample size.
    pub sample: usize,
    /// Acceptance number.
    pub acceptance: usize,
    /// Acceptance probability at the AQL.
    pub pa_aql: f64,
    /// Acceptance probability at the LTPD.
    pub pa_ltpd: f64,
}

/// Plans the smallest single-sampling scheme for `(AQL, LTPD, α, β)`.
///
/// The search walks the acceptance number from zero and, for each one, takes
/// the smallest sample size that meets both risks with binomial probabilities
/// (large-lot approximation). The overall smallest sample wins.
pub fn plan(aql: f64, ltpd: f64, alpha: f64, beta: f64) -> Result<SamplingPlan> {
    if !(0.0..1.0).contains(&aql) || !(0.0..1.0).contains(&ltpd) || aql >= ltpd {
        return Err(Error::Unsupported(
            "the AQL must be below the LTPD and both inside (0, 1)".to_owned(),
        ));
    }
    if !(0.0..1.0).contains(&alpha) || !(0.0..1.0).contains(&beta) {
        return Err(Error::InvalidParameter {
            name: "risk",
            value: alpha,
        });
    }
    let mut best: Option<SamplingPlan> = None;
    for acceptance in 0..=60usize {
        // P(accept | LTPD) falls with the sample size, so the smallest sample
        // that meets the consumer risk is found by bisection. That sample also
        // has the largest producer protection for the acceptance number, so it
        // only remains to check the AQL side.
        let mut low = acceptance + 1;
        let mut high = 5_000usize;
        if binomial_pa(low, acceptance, ltpd) <= beta {
            // Already accepted at the smallest possible sample.
        } else if binomial_pa(high, acceptance, ltpd) > beta {
            continue;
        }
        while low < high {
            let middle = (low + high) / 2;
            if binomial_pa(middle, acceptance, ltpd) <= beta {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let pa_aql = binomial_pa(low, acceptance, aql);
        let pa_ltpd = binomial_pa(low, acceptance, ltpd);
        if pa_aql < 1.0 - alpha || pa_ltpd > beta {
            continue;
        }
        let candidate = SamplingPlan {
            sample: low,
            acceptance,
            pa_aql,
            pa_ltpd,
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.sample < current.sample)
        {
            best = Some(candidate);
        }
    }
    best.ok_or_else(|| {
        Error::Unsupported(
            "no single-sampling plan meets the two risks within 5000 items".to_owned(),
        )
    })
}

/// Line plot of the OC curve.
fn plot(model: &OcCurve) -> PlotSpec {
    let points: Vec<Point> = model
        .proportions
        .iter()
        .zip(&model.probabilities)
        .map(|(p, pa)| Point { x: *p, y: *pa })
        .collect();
    let mut lines = Vec::new();
    for pair in points.windows(2) {
        lines.push(Segment {
            from: pair[0],
            to: pair[1],
            width: 2.0,
        });
    }
    PlotSpec {
        title: "Operating Characteristic Curve".to_owned(),
        x_axis: linear_axis(
            "Defective rate",
            0.0,
            model.proportions.last().copied().unwrap_or(0.5),
        ),
        y_axis: linear_axis("Probability of acceptance", 0.0, 1.0),
        size: (900, 540),
        points,
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Runs the analysis and builds the report.
pub fn analyze(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
) -> Result<AnalysisOutput> {
    let model = oc_curve(lot_size, sample, acceptance, 100)?;
    let model_name = if lot_size.is_some() {
        "Hypergeometric (sampling without replacement)"
    } else {
        "Binomial (large lot)"
    };
    let mut result = AnalysisResult::new(
        "Acceptance Sampling — OC Curve".to_owned(),
        format!("Single sampling, n = {sample}, c = {acceptance}; {model_name}"),
    );
    result.n = sample;
    let mut statistics = vec![
        ("Sample size", sample as f64),
        ("Acceptance number", acceptance as f64),
        (
            "P(accept) at 1%",
            acceptance_probability(lot_size, sample, acceptance, 0.01),
        ),
        (
            "P(accept) at 2%",
            acceptance_probability(lot_size, sample, acceptance, 0.02),
        ),
        (
            "P(accept) at 5%",
            acceptance_probability(lot_size, sample, acceptance, 0.05),
        ),
    ];
    if let Some(lot) = lot_size {
        statistics.push(("Lot size", lot as f64));
    }
    if let Some((p, aoq)) = model.aoql {
        statistics.push(("AOQL rate", p));
        statistics.push(("AOQL", aoq));
    }
    result.statistics = statistics;

    result.extra.push(Item::Table {
        headers: vec!["Defective rate".to_owned(), "P(accept)".to_owned()],
        rows: model
            .proportions
            .iter()
            .zip(&model.probabilities)
            .step_by(5)
            .map(|(p, pa)| vec![format!("{p:.4}"), format!("{pa:.6}")])
            .collect(),
    });
    result.notes = vec![
        (
            "Plan".to_owned(),
            "take n items and accept the lot when the number of defectives is at most c".to_owned(),
        ),
        (
            "Probability".to_owned(),
            "P(accept) = P(X ≤ c) with the binomial law for large lots and the hypergeometric \
             law for finite lots (defectives rounded from the rate)"
                .to_owned(),
        ),
        (
            "AOQL".to_owned(),
            "maximum of p·P(accept) (times (N − n)/N for finite lots) over the curve".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "the lot-size and rounding conventions are standard choices and have not been verified \
         against captured output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

/// Plans a scheme and builds the report with its OC curve.
pub fn analyze_plan(
    aql: f64,
    ltpd: f64,
    alpha: f64,
    beta: f64,
    lot: Option<usize>,
) -> Result<AnalysisOutput> {
    let plan = plan(aql, ltpd, alpha, beta)?;
    let model = oc_curve(None, plan.sample, plan.acceptance, 100)?;
    let mut result = AnalysisResult::new(
        "Acceptance Sampling — Plan".to_owned(),
        format!(
            "Smallest single-sampling plan for AQL {:.2}%, LTPD {:.2}%, producer risk {:.0}% \
             and consumer risk {:.0}%",
            aql * 100.0,
            ltpd * 100.0,
            alpha * 100.0,
            beta * 100.0
        ),
    );
    result.n = plan.sample;
    result.statistics = vec![
        ("Sample size", plan.sample as f64),
        ("Acceptance number", plan.acceptance as f64),
        ("P(accept) at AQL", plan.pa_aql),
        ("P(accept) at LTPD", plan.pa_ltpd),
        ("Producer risk", 1.0 - plan.pa_aql),
        ("Consumer risk", plan.pa_ltpd),
    ];
    result.notes = vec![
        (
            "Decision".to_owned(),
            "the smallest sample size that keeps P(accept) above 1 − α at the AQL and below β at \
             the LTPD"
                .to_owned(),
        ),
        (
            "Search".to_owned(),
            "every acceptance number up to 60 is tried and the sample is found by bisection"
                .to_owned(),
        ),
    ];
    // Average outgoing quality and average total inspection need the lot size;
    // the captured run used 1000 and prints both at the AQL and the LTPD.
    if let Some(lot) = lot.filter(|lot| *lot > plan.sample) {
        let n = plan.sample as f64;
        let big_n = lot as f64;
        let aoq = |p: f64, pa: f64| 100.0 * p * pa * (big_n - n) / big_n;
        let ati = |pa: f64| n + (1.0 - pa) * (big_n - n);
        result
            .statistics
            .push(("AOQ at AQL (%)", aoq(aql, plan.pa_aql)));
        result
            .statistics
            .push(("AOQ at LTPD (%)", aoq(ltpd, plan.pa_ltpd)));
        result.statistics.push(("ATI at AQL", ati(plan.pa_aql)));
        result.statistics.push(("ATI at LTPD", ati(plan.pa_ltpd)));
        let mut best = (0.0_f64, 0.0_f64);
        for step in 1..=4000 {
            let p = 0.0005 * f64::from(step);
            let pa = binomial_pa(plan.sample, plan.acceptance, p);
            let value = aoq(p, pa);
            if value > best.0 {
                best = (value, p);
            }
        }
        // Refine the coarse grid by golden-section search around it.
        let evaluate = |p: f64| aoq(p, binomial_pa(plan.sample, plan.acceptance, p));
        let mut low = (best.1 - 0.0005).max(0.0);
        let mut high = best.1 + 0.0005;
        let ratio = 0.618_033_988_749_894_9;
        for _ in 0..60 {
            let a = high - ratio * (high - low);
            let b = low + ratio * (high - low);
            if evaluate(a) > evaluate(b) {
                high = b;
            } else {
                low = a;
            }
        }
        let rate = 0.5 * (low + high);
        best = (evaluate(rate), rate);
        result.statistics.push(("AOQL (%)", best.0));
        result.statistics.push(("AOQL rate (%)", best.1 * 100.0));
        result.notes.push((
            "Lot size".to_owned(),
            format!("{lot}: AOQ and ATI use (N − n)/N for the rectifying inspection"),
        ));
    }
    result.warnings = standard_warnings(result.n, 0, 0);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binomial_curve_matches_the_definition() {
        // n = 20, c = 1, p = 0.05: 0.95^20 + 20·0.05·0.95^19.
        let expected = 0.95f64.powi(20) + 20.0 * 0.05 * 0.95f64.powi(19);
        let pa = binomial_pa(20, 1, 0.05);
        assert!((pa - expected).abs() < 1e-10, "{pa} vs {expected}");
        assert!((binomial_pa(20, 1, 0.0) - 1.0).abs() < 1e-12);
        assert!((binomial_pa(20, 1, 1.0)).abs() < 1e-12);
        assert!((binomial_pa(20, 20, 0.5) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_hypergeometric_curve_matches_the_definition() {
        // N = 100, D = 10, n = 20, c = 0: only the 90 good items may be drawn.
        let pa = hypergeometric_pa(100, 10, 20, 0);
        let mut expected = 1.0;
        for index in 0..20 {
            expected *= (90.0 - index as f64) / (100.0 - index as f64);
        }
        assert!((pa - expected).abs() < 1e-10, "{pa} vs {expected}");
        // A plan that accepts anything accepts every lot.
        assert!((hypergeometric_pa(100, 10, 20, 20) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_curve_falls_with_the_defective_rate() {
        let curve = oc_curve(None, 30, 1, 60).expect("curve");
        assert_eq!(curve.proportions.len(), 61);
        for pair in curve.probabilities.windows(2) {
            assert!(pair[0] >= pair[1] - 1e-12);
        }
        assert!((curve.probabilities[0] - 1.0).abs() < 1e-12);
        assert!(curve.probabilities.last().copied().unwrap_or(1.0) < 0.01);
        let (_, aoql) = curve.aoql.expect("AOQL");
        assert!((0.0..=1.0).contains(&aoql));
    }

    #[test]
    fn the_plan_meets_both_risks() {
        let scheme = plan(0.01, 0.05, 0.05, 0.10).expect("plan");
        assert!(scheme.sample > scheme.acceptance);
        assert!(scheme.pa_aql >= 0.95 - 1e-9, "{scheme:?}");
        assert!(scheme.pa_ltpd <= 0.10 + 1e-9, "{scheme:?}");
        // A smaller sample with the same acceptance number breaks a risk.
        if scheme.sample > scheme.acceptance + 1 {
            let smaller = scheme.sample - 1;
            let good = binomial_pa(smaller, scheme.acceptance, 0.01) >= 0.95
                && binomial_pa(smaller, scheme.acceptance, 0.05) <= 0.10;
            assert!(!good, "sample {smaller} also works");
        }
        assert!(matches!(
            plan(0.05, 0.01, 0.05, 0.10),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_plan_matches_the_captured_aoq_and_ati() {
        // Captured: AQL 1%, LTPD 5%, risks 5%/10%, lot 1000 -> n = 132, c = 3,
        // AOQ 0.830% / 0.431%, ATI 170.4 / 913.9, AOQL 1.278% at 2.220%.
        let output = analyze_plan(0.01, 0.05, 0.05, 0.10, Some(1000)).expect("plan");
        let text = output.results[0].to_session_text();
        let value = |name: &str| {
            output.results[0]
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("{name} missing in {text}"))
        };
        assert_eq!(value("Sample size"), 132.0);
        assert_eq!(value("Acceptance number"), 3.0);
        assert!((value("AOQ at AQL (%)") - 0.830).abs() < 0.01);
        assert!((value("AOQ at LTPD (%)") - 0.431).abs() < 0.01);
        assert!((value("ATI at AQL") - 170.4).abs() < 0.2);
        assert!((value("ATI at LTPD") - 913.9).abs() < 0.2);
        assert!((value("AOQL (%)") - 1.278).abs() < 0.01);
        assert!((value("AOQL rate (%)") - 2.220).abs() < 0.02);
    }

    #[test]
    fn the_reports_and_the_plots_are_built() {
        let out = analyze(Some(500), 20, 1).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Acceptance Sampling"), "{text}");
        assert!(text.contains("Hypergeometric"), "{text}");
        assert!(text.contains("AOQL"), "{text}");
        assert!(out.plot.is_some());

        let out = analyze_plan(0.01, 0.05, 0.05, 0.10, None).expect("plan report");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Plan"), "{text}");
        assert!(text.contains("Producer risk"), "{text}");
    }
}
