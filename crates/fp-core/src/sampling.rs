// SPDX-License-Identifier: MIT
//! Acceptance sampling by attributes: operating-characteristic curves.
//!
//! A **single-sampling** plan takes `n` items from a lot and accepts it when
//! the number of defectives is at most `c`. The **probability of acceptance**
//! of a lot with defective rate `p` is `P(X ≤ c)`:
//!
//! - **large (or infinite) lots**: `X ~ Binomial(n, p)`;
//! - **finite lots**: `X ~ Hypergeometric(N, D, n)` with `D = round(p·N)`,
//!   because sampling without replacement changes the probability.
//!
//! A **double-sampling** plan takes a first sample `n₁`, accepts at once when
//! its defectives are at most `c₁`, rejects at once when they are at least
//! `r₁`, and otherwise takes a second sample `n₂` and accepts when the two
//! samples together hold at most `c₂` defectives. Its acceptance probability
//! adds the immediate acceptance to every continuation branch:
//!
//! `Pa(p) = P(D₁ ≤ c₁) + Σ P(D₁ = d₁) · P(D₂ ≤ c₂ − d₁)`.
//!
//! The report draws the **OC curve** `Pa(p)`, gives the acceptance probability
//! at the AQL and the LTPD with the producer and consumer risks, the average
//! sample number (`ASN`), the average outgoing quality (`AOQ`) and the average
//! total inspection (`ATI`). `AOQ` and `ATI` use the lot size when it is known;
//! without a lot size the work is the large-lot (binomial) case and `AOQ` is
//! `p·Pa(p)` while `ATI` does not exist.
//!
//! When asked, the module **plans a single-sampling scheme** for a pair
//! `(AQL, LTPD)` with the producer and consumer risks: the smallest sample that
//! gives `Pa(AQL) ≥ 1 − α` and `Pa(LTPD) ≤ β`.

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
    /// Sample size of a single plan, or the first sample of a double plan.
    pub sample: usize,
    /// Acceptance number of a single plan, or the first acceptance number.
    pub acceptance: usize,
    /// The double-sampling plan, when the curve belongs to one.
    pub double: Option<DoublePlan>,
    /// Defective rates of the curve.
    pub proportions: Vec<f64>,
    /// Acceptance probability at each rate.
    pub probabilities: Vec<f64>,
    /// Average sample number at each rate.
    pub asn: Vec<f64>,
    /// Average outgoing quality at each rate.
    pub aoq: Vec<f64>,
    /// Average total inspection at each rate, when the lot size is known.
    pub ati: Option<Vec<f64>>,
    /// Average outgoing quality limit of the plan.
    pub aoql: Option<(f64, f64)>,
}

/// A double-sampling plan by attributes.
///
/// The first sample accepts at `c₁`, rejects at `r₁` and continues in between;
/// the second sample accepts when the total defectives are at most `c₂`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoublePlan {
    /// Items in the first sample (`n₁`).
    pub first_sample: usize,
    /// Accept the lot when the first sample holds at most this many defectives
    /// (`c₁`).
    pub first_accept: usize,
    /// Reject the lot when the first sample holds at least this many
    /// defectives (`r₁`).
    pub first_reject: usize,
    /// Items in the second sample (`n₂`).
    pub second_sample: usize,
    /// Accept when both samples together hold at most this many defectives
    /// (`c₂`).
    pub second_accept: usize,
}

impl DoublePlan {
    /// Validates and builds a double-sampling plan.
    ///
    /// # Errors
    ///
    /// Returns the reason when a sample is empty, the acceptance number is
    /// not below the rejection number, or the acceptance number of the second
    /// sample does not leave room for a rejection.
    pub fn new(
        first_sample: usize,
        first_accept: usize,
        first_reject: usize,
        second_sample: usize,
        second_accept: usize,
    ) -> Result<Self> {
        if first_sample == 0 || second_sample == 0 {
            return Err(Error::InvalidParameter {
                name: "sample size",
                value: 0.0,
            });
        }
        if first_accept >= first_reject {
            return Err(Error::Unsupported(
                "the first acceptance number must be below the first rejection number".to_owned(),
            ));
        }
        if first_reject > first_sample {
            return Err(Error::Unsupported(
                "the first rejection number must not exceed the first sample".to_owned(),
            ));
        }
        if second_accept <= first_accept {
            return Err(Error::Unsupported(
                "the second acceptance number must be above the first one".to_owned(),
            ));
        }
        if first_sample
            .checked_add(second_sample)
            .is_none_or(|total| second_accept >= total)
        {
            return Err(Error::Unsupported(
                "the total acceptance number must be below both samples together".to_owned(),
            ));
        }
        Ok(Self {
            first_sample,
            first_accept,
            first_reject,
            second_sample,
            second_accept,
        })
    }

    /// Smallest and largest number of items the plan inspects.
    #[must_use]
    pub fn sample_range(&self) -> (usize, usize) {
        (self.first_sample, self.first_sample + self.second_sample)
    }
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

/// `P(X = d)` for `X ~ Binomial(n, p)`.
fn binomial_pmf(sample: usize, defective: usize, p: f64) -> f64 {
    if defective > sample {
        return 0.0;
    }
    if p <= 0.0 {
        return if defective == 0 { 1.0 } else { 0.0 };
    }
    if p >= 1.0 {
        return if defective == sample { 1.0 } else { 0.0 };
    }
    let n = sample as f64;
    let k = defective as f64;
    (ln_gamma(n + 1.0) - ln_gamma(k + 1.0) - ln_gamma(n - k + 1.0)
        + k * p.ln()
        + (n - k) * (1.0 - p).ln())
    .exp()
}

/// `P(X = d)` for `X ~ Hypergeometric(N, D, n)`.
fn hypergeometric_pmf(lot_size: usize, defectives: usize, sample: usize, defective: usize) -> f64 {
    if lot_size == 0 || sample > lot_size || defectives > lot_size {
        return f64::NAN;
    }
    let good = lot_size - defectives;
    if defective > defectives || sample < defective || sample - defective > good {
        return 0.0;
    }
    let log_term = ln_gamma(defectives as f64 + 1.0)
        - ln_gamma(defective as f64 + 1.0)
        - ln_gamma((defectives - defective) as f64 + 1.0)
        + ln_gamma(good as f64 + 1.0)
        - ln_gamma((sample - defective) as f64 + 1.0)
        - ln_gamma((good - (sample - defective)) as f64 + 1.0);
    let log_denominator = ln_gamma(lot_size as f64 + 1.0)
        - ln_gamma(sample as f64 + 1.0)
        - ln_gamma((lot_size - sample) as f64 + 1.0);
    (log_term - log_denominator).exp()
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
    let highest = acceptance.min(sample).min(defectives);
    let mut sum = 0.0;
    for defective in 0..=highest {
        sum += hypergeometric_pmf(lot_size, defectives, sample, defective);
    }
    sum.clamp(0.0, 1.0)
}

/// Acceptance probability of a double plan with the binomial law.
#[must_use]
pub fn double_binomial_pa(plan: &DoublePlan, p: f64) -> f64 {
    let first = binomial_pa(plan.first_sample, plan.first_accept, p);
    let last_continuation = plan.first_reject.min(plan.first_sample + 1);
    let mut second = 0.0;
    for first_defectives in (plan.first_accept + 1)..last_continuation {
        let probability = binomial_pmf(plan.first_sample, first_defectives, p);
        if probability == 0.0 {
            continue;
        }
        let allowance = plan.second_accept.saturating_sub(first_defectives);
        second += probability * binomial_pa(plan.second_sample, allowance, p);
    }
    (first + second).clamp(0.0, 1.0)
}

/// Acceptance probability of a double plan with the hypergeometric law.
///
/// The second sample is drawn without replacement from what remains of the
/// lot after the first sample, so both its lot size and its defectives fall by
/// the first-sample counts.
#[must_use]
pub fn double_hypergeometric_pa(lot_size: usize, defectives: usize, plan: &DoublePlan) -> f64 {
    let (first_sample, second_sample) = (plan.first_sample, plan.second_sample);
    if lot_size == 0 || first_sample + second_sample > lot_size {
        return f64::NAN;
    }
    let defectives = defectives.min(lot_size);
    let first = hypergeometric_pa(lot_size, defectives, first_sample, plan.first_accept);
    let last_continuation = plan.first_reject.min(first_sample + 1);
    let remaining_lot = lot_size - first_sample;
    let mut second = 0.0;
    for first_defectives in (plan.first_accept + 1)..last_continuation {
        let probability = hypergeometric_pmf(lot_size, defectives, first_sample, first_defectives);
        if probability == 0.0 {
            continue;
        }
        let remaining_defectives = defectives.saturating_sub(first_defectives);
        let allowance = plan.second_accept.saturating_sub(first_defectives);
        second += probability
            * hypergeometric_pa(
                remaining_lot,
                remaining_defectives,
                second_sample,
                allowance,
            );
    }
    (first + second).clamp(0.0, 1.0)
}

/// Acceptance probability of a double plan at a defective rate.
#[must_use]
pub fn double_pa(lot_size: Option<usize>, plan: &DoublePlan, p: f64) -> f64 {
    match lot_size {
        Some(lot) => {
            let defectives = (p * lot as f64).round().clamp(0.0, lot as f64) as usize;
            double_hypergeometric_pa(lot, defectives, plan)
        }
        None => double_binomial_pa(plan, p),
    }
}

/// Acceptance probability of a single plan at a defective rate.
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

/// Probability that a double plan needs the second sample.
fn second_sample_share(lot_size: Option<usize>, plan: &DoublePlan, p: f64) -> f64 {
    let last_continuation = plan.first_reject.saturating_sub(1).min(plan.first_sample);
    match lot_size {
        Some(lot) => {
            let defectives = (p * lot as f64).round().clamp(0.0, lot as f64) as usize;
            (hypergeometric_pa(lot, defectives, plan.first_sample, last_continuation)
                - hypergeometric_pa(lot, defectives, plan.first_sample, plan.first_accept))
            .clamp(0.0, 1.0)
        }
        None => (binomial_pa(plan.first_sample, last_continuation, p)
            - binomial_pa(plan.first_sample, plan.first_accept, p))
        .clamp(0.0, 1.0),
    }
}

/// Assembles the curve, the average sample number and the quality measures.
fn finish_curve(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
    double: Option<DoublePlan>,
    proportions: Vec<f64>,
    probabilities: Vec<f64>,
    asn: Vec<f64>,
) -> OcCurve {
    let aoq: Vec<f64> = proportions
        .iter()
        .zip(&probabilities)
        .zip(&asn)
        .map(|((p, pa), asn)| match lot_size {
            Some(lot) => p * pa * ((lot as f64 - asn) / lot as f64),
            None => p * pa,
        })
        .collect();
    let ati = lot_size.map(|lot| {
        asn.iter()
            .zip(&probabilities)
            .map(|(asn, pa)| asn + (1.0 - pa) * (lot as f64 - asn))
            .collect()
    });
    let mut aoql: Option<(f64, f64)> = None;
    for (p, quality) in proportions.iter().zip(&aoq) {
        if aoql.is_none_or(|(_, best)| *quality > best) {
            aoql = Some((*p, *quality));
        }
    }
    OcCurve {
        lot_size,
        sample,
        acceptance,
        double,
        proportions,
        probabilities,
        asn,
        aoq,
        ati,
        aoql,
    }
}

/// Builds the OC curve of a single-sampling plan.
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
    let asn = vec![sample as f64; proportions.len()];
    Ok(finish_curve(
        lot_size,
        sample,
        acceptance,
        None,
        proportions,
        probabilities,
        asn,
    ))
}

/// Builds the OC curve of a double-sampling plan.
pub fn double_oc_curve(lot_size: Option<usize>, plan: DoublePlan, steps: usize) -> Result<OcCurve> {
    let (first_sample, second_sample) = plan.sample_range();
    if let Some(lot) = lot_size
        && (lot == 0 || second_sample > lot)
    {
        return Err(Error::Unsupported(
            "both samples together must not exceed the lot size".to_owned(),
        ));
    }
    let steps = steps.clamp(10, 400);
    let proportions: Vec<f64> = (0..=steps)
        .map(|index| index as f64 / steps as f64 * 0.5)
        .collect();
    let probabilities: Vec<f64> = proportions
        .iter()
        .map(|p| double_pa(lot_size, &plan, *p))
        .collect();
    let asn: Vec<f64> = proportions
        .iter()
        .map(|p| {
            first_sample as f64 + second_sample as f64 * second_sample_share(lot_size, &plan, *p)
        })
        .collect();
    Ok(finish_curve(
        lot_size,
        first_sample,
        plan.first_accept,
        Some(plan),
        proportions,
        probabilities,
        asn,
    ))
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

/// Validates an optional `(AQL, LTPD)` pair.
fn validate_risks(aql: Option<f64>, ltpd: Option<f64>) -> Result<()> {
    match (aql, ltpd) {
        (None, None) => Ok(()),
        (Some(aql), Some(ltpd)) => {
            if !(0.0..1.0).contains(&aql) || !(0.0..1.0).contains(&ltpd) || aql >= ltpd {
                return Err(Error::Unsupported(
                    "the AQL must be below the LTPD and both inside (0, 1)".to_owned(),
                ));
            }
            Ok(())
        }
        _ => Err(Error::Unsupported(
            "give both the AQL and the LTPD or neither".to_owned(),
        )),
    }
}

/// Adds the acceptance probability, risks, ASN, AOQ and ATI at one point.
fn push_rate_statistics(
    result: &mut AnalysisResult,
    lot_size: Option<usize>,
    rate: f64,
    probability: f64,
    inspected: f64,
    quality: bool,
) {
    let scale = lot_size.map_or(1.0, |lot| (lot as f64 - inspected) / lot as f64);
    let aoq = 100.0 * rate * probability * scale;
    let ati = lot_size.map(|lot| inspected + (1.0 - probability) * (lot as f64 - inspected));
    if quality {
        result.statistics.push(("P(accept) at AQL", probability));
        result.statistics.push(("Producer risk", 1.0 - probability));
        result.statistics.push(("ASN at AQL", inspected));
        result.statistics.push(("AOQ at AQL (%)", aoq));
        if let Some(ati) = ati {
            result.statistics.push(("ATI at AQL", ati));
        }
    } else {
        result.statistics.push(("P(accept) at LTPD", probability));
        result.statistics.push(("Consumer risk", probability));
        result.statistics.push(("ASN at LTPD", inspected));
        result.statistics.push(("AOQ at LTPD (%)", aoq));
        if let Some(ati) = ati {
            result.statistics.push(("ATI at LTPD", ati));
        }
    }
}

/// Runs a single-sampling analysis and builds the report.
pub fn analyze(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
) -> Result<AnalysisOutput> {
    analyze_single_with_risks(lot_size, sample, acceptance, None, None)
}

/// Runs a single-sampling analysis with optional AQL and LTPD points.
pub fn analyze_single_with_risks(
    lot_size: Option<usize>,
    sample: usize,
    acceptance: usize,
    aql: Option<f64>,
    ltpd: Option<f64>,
) -> Result<AnalysisOutput> {
    validate_risks(aql, ltpd)?;
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
    if let (Some(aql), Some(ltpd)) = (aql, ltpd) {
        for (rate, quality) in [(aql, true), (ltpd, false)] {
            push_rate_statistics(
                &mut result,
                lot_size,
                rate,
                acceptance_probability(lot_size, sample, acceptance, rate),
                sample as f64,
                quality,
            );
        }
    }

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
    if aql.is_some() {
        result.notes.push((
            "Risks".to_owned(),
            "the producer risk is 1 − P(accept) at the AQL; the consumer risk is P(accept) at \
             the LTPD. AOQ and ATI describe rectifying inspection and use (N − n)/N"
                .to_owned(),
        ));
    }
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "the lot-size and rounding conventions are standard choices and have not been verified \
         against documented output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot(&model)),
    })
}

/// Runs a double-sampling analysis and builds the report.
pub fn analyze_double(
    lot_size: Option<usize>,
    plan: DoublePlan,
    aql: Option<f64>,
    ltpd: Option<f64>,
) -> Result<AnalysisOutput> {
    validate_risks(aql, ltpd)?;
    let model = double_oc_curve(lot_size, plan, 100)?;
    let model_name = if lot_size.is_some() {
        "Hypergeometric (sampling without replacement)"
    } else {
        "Binomial (large lot)"
    };
    let mut result = AnalysisResult::new(
        "Acceptance Sampling — Double OC Curve".to_owned(),
        format!(
            "Double sampling, n₁ = {}, c₁ = {}, r₁ = {}, n₂ = {}, c₂ = {}; {model_name}",
            plan.first_sample,
            plan.first_accept,
            plan.first_reject,
            plan.second_sample,
            plan.second_accept
        ),
    );
    result.n = plan.first_sample + plan.second_sample;
    let mut statistics = vec![
        ("First sample", plan.first_sample as f64),
        ("First acceptance", plan.first_accept as f64),
        ("First rejection", plan.first_reject as f64),
        ("Second sample", plan.second_sample as f64),
        ("Second acceptance", plan.second_accept as f64),
        ("P(accept) at 1%", double_pa(lot_size, &plan, 0.01)),
        ("P(accept) at 2%", double_pa(lot_size, &plan, 0.02)),
        ("P(accept) at 5%", double_pa(lot_size, &plan, 0.05)),
        (
            "ASN at 1%",
            plan.first_sample as f64
                + plan.second_sample as f64 * second_sample_share(lot_size, &plan, 0.01),
        ),
        (
            "ASN at 2%",
            plan.first_sample as f64
                + plan.second_sample as f64 * second_sample_share(lot_size, &plan, 0.02),
        ),
        (
            "ASN at 5%",
            plan.first_sample as f64
                + plan.second_sample as f64 * second_sample_share(lot_size, &plan, 0.05),
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
    if let (Some(aql), Some(ltpd)) = (aql, ltpd) {
        for (rate, quality) in [(aql, true), (ltpd, false)] {
            let inspected = plan.first_sample as f64
                + plan.second_sample as f64 * second_sample_share(lot_size, &plan, rate);
            push_rate_statistics(
                &mut result,
                lot_size,
                rate,
                double_pa(lot_size, &plan, rate),
                inspected,
                quality,
            );
        }
    }

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
            "after the first sample, accept when the defectives are at most c₁, reject when they \
             are at least r₁, and otherwise take the second sample and accept when the two \
             samples together hold at most c₂ defectives"
                .to_owned(),
        ),
        (
            "Probability".to_owned(),
            "P(accept) = P(D₁ ≤ c₁) + Σ P(D₁ = d₁)·P(D₂ ≤ c₂ − d₁) over the continuation \
             d₁ = c₁+1 … r₁−1, with the binomial law for large lots and the hypergeometric law \
             for finite lots (defectives rounded from the rate)"
                .to_owned(),
        ),
        (
            "ASN".to_owned(),
            "average sample number: n₁ plus n₂ times the probability that the first sample falls \
             between c₁ and r₁"
                .to_owned(),
        ),
        (
            "AOQL".to_owned(),
            "maximum of p·P(accept)·(N − ASN)/N for finite lots and of p·P(accept) otherwise; \
             AOQ and ATI describe rectifying inspection and need the lot size for ATI"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, 0, 0);
    result.warnings.push(
        "the lot-size, rounding and ASN conventions are standard choices and have not been \
         verified against documented output"
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
    // the documented behaviour used 1000 and prints both at the AQL and the LTPD.
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

    /// Reference binomial pmf from the combinatorial definition, computed
    /// with a product so it is independent of `ln_gamma`.
    fn reference_binomial_pmf(sample: usize, defective: usize, p: f64) -> f64 {
        if defective > sample {
            return 0.0;
        }
        let mut combination = 1.0;
        for index in 0..defective {
            combination *= (sample - index) as f64 / (index + 1) as f64;
        }
        combination * p.powi(defective as i32) * (1.0 - p).powi((sample - defective) as i32)
    }

    /// Reference hypergeometric pmf from the combinatorial definition.
    fn reference_hypergeometric_pmf(
        lot_size: usize,
        defectives: usize,
        sample: usize,
        defective: usize,
    ) -> f64 {
        fn combination(n: usize, k: usize) -> f64 {
            if k > n {
                return 0.0;
            }
            let mut value = 1.0;
            for index in 0..k {
                value *= (n - index) as f64 / (index + 1) as f64;
            }
            value
        }
        if defective > defectives || sample - defective > lot_size - defectives {
            return 0.0;
        }
        combination(defectives, defective) * combination(lot_size - defectives, sample - defective)
            / combination(lot_size, sample)
    }

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
    fn the_oc_table_probability_for_fifty_items() {
        // Standard OC table: n = 50, c = 1, p = 0.01 -> Pa = 0.9106.
        let pa = binomial_pa(50, 1, 0.01);
        let reference: f64 = (0..=1)
            .map(|defective| reference_binomial_pmf(50, defective, 0.01))
            .sum();
        assert!((pa - 0.910_564_686_9).abs() < 1e-9, "{pa}");
        assert!((pa - reference).abs() < 1e-12, "{pa} vs {reference}");
    }

    #[test]
    fn the_oc_table_probability_for_one_hundred_items() {
        // Standard OC table: n = 100, c = 2, p = 0.02 -> Pa = 0.6767.
        let pa = binomial_pa(100, 2, 0.02);
        let reference: f64 = (0..=2)
            .map(|defective| reference_binomial_pmf(100, defective, 0.02))
            .sum();
        assert!((pa - 0.676_685_622_4).abs() < 1e-9, "{pa}");
        assert!((pa - reference).abs() < 1e-12, "{pa} vs {reference}");
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
    fn the_double_plan_probability_matches_the_definition() {
        let plan = DoublePlan::new(50, 1, 4, 50, 4).expect("plan");
        for p in [0.01, 0.02, 0.05] {
            let expected = (0..=1)
                .map(|first| reference_binomial_pmf(50, first, p))
                .sum::<f64>()
                + (2..=3)
                    .map(|first| {
                        reference_binomial_pmf(50, first, p)
                            * (0..=(4 - first))
                                .map(|second| reference_binomial_pmf(50, second, p))
                                .sum::<f64>()
                    })
                    .sum::<f64>();
            let actual = double_binomial_pa(&plan, p);
            assert!(
                (actual - expected).abs() < 1e-12,
                "p={p}: {actual} vs {expected}"
            );
        }
        // Reference value: n₁ = 50, c₁ = 1, r₁ = 4, n₂ = 50, c₂ = 4 at 2 %.
        let pa = double_binomial_pa(&plan, 0.02);
        assert!((pa - 0.951_639_314_7).abs() < 1e-9, "{pa}");
        // The plan accepts more often than its first sample alone.
        assert!(pa > binomial_pa(50, 1, 0.02));
    }

    #[test]
    fn the_double_hypergeometric_probability_matches_the_definition() {
        let plan = DoublePlan::new(50, 1, 4, 50, 4).expect("plan");
        let (lot, defectives) = (500, 10);
        let expected = (0..=1)
            .map(|first| reference_hypergeometric_pmf(lot, defectives, 50, first))
            .sum::<f64>()
            + (2..=3)
                .map(|first| {
                    reference_hypergeometric_pmf(lot, defectives, 50, first)
                        * (0..=(4 - first))
                            .map(|second| {
                                reference_hypergeometric_pmf(
                                    lot - 50,
                                    defectives - first,
                                    50,
                                    second,
                                )
                            })
                            .sum::<f64>()
                })
                .sum::<f64>();
        let actual = double_hypergeometric_pa(lot, defectives, &plan);
        assert!((actual - expected).abs() < 1e-12, "{actual} vs {expected}");
        assert!((actual - 0.968_706_991_8).abs() < 1e-9, "{actual}");
        // Finite lots are easier to accept than the binomial prediction when
        // the lot is small: the second sample is drawn without replacement.
        assert!(actual > 0.95);
    }

    #[test]
    fn the_double_curve_reports_asn_and_outgoing_quality() {
        let plan = DoublePlan::new(50, 1, 4, 50, 4).expect("plan");
        // 2 % is index 4 on the 0.005 grid.
        let index = 4;

        // Large lot: binomial. Pa = 0.9516393147, ASN = 62.32352624 and
        // AOQ = p·Pa = 0.0190327863.
        let unlimited = double_oc_curve(None, plan, 100).expect("curve");
        let p = unlimited.proportions[index];
        assert!((p - 0.02).abs() < 1e-12);
        let pa = unlimited.probabilities[index];
        assert!((pa - 0.951_639_314_7).abs() < 1e-9, "{pa}");
        let asn = unlimited.asn[index];
        assert!((asn - 62.323_526_24).abs() < 1e-6, "{asn}");
        assert!((unlimited.aoq[index] - p * pa).abs() < 1e-12);
        assert!(unlimited.ati.is_none());
        assert!(
            unlimited.asn.iter().all(|asn| (50.0..=100.0).contains(asn)),
            "{:?}",
            unlimited.asn
        );

        // Finite lot N = 1000 with D = 20: hypergeometric. Pa = 0.9598402751,
        // ASN = 62.45742984, AOQ = 0.01799782237 and ATI = 100.10888153.
        let curve = double_oc_curve(Some(1000), plan, 100).expect("curve");
        assert!((curve.probabilities[index] - 0.959_840_275_1).abs() < 1e-9);
        assert!((curve.asn[index] - 62.457_429_84).abs() < 1e-6);
        assert!((curve.aoq[index] - 0.017_997_822_4).abs() < 1e-9);
        let ati = curve.ati.as_ref().expect("ATI with a lot")[index];
        assert!((ati - 100.108_881_5).abs() < 1e-6, "{ati}");
    }

    #[test]
    fn the_double_report_gives_the_aql_and_ltpd_points() {
        let plan = DoublePlan::new(50, 1, 4, 50, 4).expect("plan");
        let output = analyze_double(Some(1000), plan, Some(0.01), Some(0.05)).expect("report");
        let value = |name: &str| {
            output.results[0]
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        let pa_aql = double_pa(Some(1000), &plan, 0.01);
        let pa_ltpd = double_pa(Some(1000), &plan, 0.05);
        assert!((value("P(accept) at AQL") - pa_aql).abs() < 1e-12);
        assert!((value("Producer risk") - (1.0 - pa_aql)).abs() < 1e-12);
        assert!((value("P(accept) at LTPD") - pa_ltpd).abs() < 1e-12);
        assert!((value("Consumer risk") - pa_ltpd).abs() < 1e-12);
        assert!(value("ATI at AQL") > value("ASN at AQL"));
        assert!(value("ATI at LTPD") > value("ASN at LTPD"));
        assert!(output.results[0].to_session_text().contains("Double"));
        assert!(output.plot.is_some());
    }

    #[test]
    fn the_single_report_adds_the_risk_points_and_the_ati() {
        let output =
            analyze_single_with_risks(Some(1000), 20, 1, Some(0.01), Some(0.05)).expect("report");
        let value = |name: &str| {
            output.results[0]
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        let pa_aql = hypergeometric_pa(1000, 10, 20, 1);
        let pa_ltpd = hypergeometric_pa(1000, 50, 20, 1);
        assert!((value("P(accept) at AQL") - pa_aql).abs() < 1e-12);
        assert!((value("Producer risk") - (1.0 - pa_aql)).abs() < 1e-12);
        assert!((value("P(accept) at LTPD") - pa_ltpd).abs() < 1e-12);
        assert!((value("Consumer risk") - pa_ltpd).abs() < 1e-12);
        assert!((value("ATI at AQL") - (20.0 + (1.0 - pa_aql) * 980.0)).abs() < 1e-9);
        assert!((value("ATI at LTPD") - (20.0 + (1.0 - pa_ltpd) * 980.0)).abs() < 1e-9);
        assert!((value("AOQ at AQL (%)") - 100.0 * 0.01 * pa_aql * 0.98).abs() < 1e-9);
        assert!(matches!(
            analyze_single_with_risks(None, 20, 1, Some(0.05), Some(0.01)),
            Err(Error::Unsupported(_))
        ));
        // The two points go together or not at all.
        assert!(matches!(
            analyze_single_with_risks(None, 20, 1, Some(0.01), None),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            analyze_double(
                None,
                DoublePlan::new(50, 1, 4, 50, 4).expect("plan"),
                None,
                Some(0.05)
            ),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn the_double_plan_rejects_impossible_numbers() {
        assert!(matches!(
            DoublePlan::new(50, 1, 1, 50, 4),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            DoublePlan::new(50, 1, 4, 0, 4),
            Err(Error::InvalidParameter { .. })
        ));
        assert!(matches!(
            DoublePlan::new(50, 1, 4, 50, 1),
            Err(Error::Unsupported(_))
        ));
        let plan = DoublePlan::new(50, 1, 4, 50, 4).expect("plan");
        assert!(matches!(
            double_oc_curve(Some(90), plan, 100),
            Err(Error::Unsupported(_))
        ));
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
    fn the_plan_matches_the_documented_aoq_and_ati() {
        // Documented: AQL 1%, LTPD 5%, risks 5%/10%, lot 1000 -> n = 132, c = 3,
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
