//! Multi-response desirability optimisation (Derringer–Suich).
//!
//! Each response is fitted with the quadratic response-surface model and its
//! predicted value is mapped to a desirability in `[0, 1]`:
//!
//! - **Maximize**: `d = ((y − low)/(high − low))^weight`.
//! - **Minimize**: `d = ((high − y)/(high − low))^weight`.
//! - **Target**: a tent that reaches 1 at the target and falls to 0 at the
//!   lower and upper bounds, with the weight applied on each side.
//!
//! The responses are combined with the weighted geometric mean
//! `D = (Π dᵢ^importanceᵢ)^(1/Σ importanceᵢ)`, and `D` is maximised over the
//! coded factor region `[−1, 1]^k` with a multi-start Nelder–Mead search.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::optimize::nelder_mead;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::rsm::{self, RsmFit};
use crate::workflow::AnalysisOutput;

/// Direction of a response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// Larger is better.
    Maximize,
    /// Smaller is better.
    Minimize,
    /// A target value is best.
    Target,
}

impl Goal {
    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Maximize => "Maximize",
            Self::Minimize => "Minimize",
            Self::Target => "Target",
        }
    }

    /// Stable identifier used by the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Maximize => "maximize",
            Self::Minimize => "minimize",
            Self::Target => "target",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "maximize" | "max" | "larger" => Some(Self::Maximize),
            "minimize" | "min" | "smaller" => Some(Self::Minimize),
            "target" | "nominal" => Some(Self::Target),
            _ => None,
        }
    }
}

/// One response in the optimiser.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// Response column.
    pub column: String,
    /// Direction.
    pub goal: Goal,
    /// Lower bound (0 desirability at or below it for Maximize/Target).
    pub low: f64,
    /// Target (used by [`Goal::Target`]).
    pub target: f64,
    /// Upper bound.
    pub high: f64,
    /// Shape weight (`1` is linear).
    pub weight: f64,
    /// Importance in the combined desirability.
    pub importance: f64,
}

impl Response {
    /// Validates the limits and the weights.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidParameter`] for non-finite limits, `low ≥ high`,
    /// a target outside the interval, or non-positive weights.
    pub fn validate(&self) -> Result<()> {
        if !self.low.is_finite() || !self.high.is_finite() || self.low >= self.high {
            return Err(Error::InvalidParameter {
                name: "desirability bounds",
                value: self.high - self.low,
            });
        }
        if self.goal == Goal::Target && !(self.target > self.low && self.target < self.high) {
            return Err(Error::InvalidParameter {
                name: "desirability target",
                value: self.target,
            });
        }
        if !self.weight.is_finite() || self.weight <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "desirability weight",
                value: self.weight,
            });
        }
        if !self.importance.is_finite() || self.importance <= 0.0 {
            return Err(Error::InvalidParameter {
                name: "desirability importance",
                value: self.importance,
            });
        }
        Ok(())
    }

    /// Desirability of one predicted value.
    #[must_use]
    pub fn desirability(&self, value: f64) -> f64 {
        if !value.is_finite() {
            return 0.0;
        }
        let span = self.high - self.low;
        match self.goal {
            Goal::Maximize => ((value - self.low) / span)
                .clamp(0.0, 1.0)
                .powf(self.weight),
            Goal::Minimize => ((self.high - value) / span)
                .clamp(0.0, 1.0)
                .powf(self.weight),
            Goal::Target => {
                if value <= self.low || value >= self.high {
                    return 0.0;
                }
                let ratio = if value <= self.target {
                    (value - self.low) / (self.target - self.low)
                } else {
                    (self.high - value) / (self.high - self.target)
                };
                ratio.clamp(0.0, 1.0).powf(self.weight)
            }
        }
    }
}

/// Result of the multi-response optimisation.
#[derive(Debug, Clone, PartialEq)]
pub struct DesirabilityFit {
    /// Factors in coded order.
    pub factors: Vec<String>,
    /// Responses.
    pub responses: Vec<Response>,
    /// Coded factor values at the optimum.
    pub optimum: Vec<f64>,
    /// Predicted response at the optimum.
    pub predicted: Vec<f64>,
    /// Individual desirability at the optimum.
    pub individual: Vec<f64>,
    /// Combined desirability at the optimum.
    pub combined: f64,
    /// Fitted models, in response order.
    pub models: Vec<RsmFit>,
}

/// Combined desirability of a set of individual values.
#[must_use]
pub fn combined(responses: &[Response], individual: &[f64]) -> f64 {
    let mut total = 0.0;
    let mut log_sum = 0.0;
    for (response, value) in responses.iter().zip(individual) {
        if *value <= 0.0 {
            return 0.0;
        }
        total += response.importance;
        log_sum += response.importance * value.ln();
    }
    if total > 0.0 {
        (log_sum / total).exp().clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Fits the responses and maximises the combined desirability.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty/duplicate response list, a
/// response used as a factor, or fewer than two factors; the validation errors
/// of [`Response::validate`] and the fit errors of the response-surface model
/// are propagated.
pub fn optimize(
    df: &DataFrame,
    factors: &[String],
    responses: &[Response],
) -> Result<DesirabilityFit> {
    if factors.len() < 2 || factors.len() > 5 {
        return Err(Error::Unsupported(
            "the response optimiser needs between two and five factors".to_owned(),
        ));
    }
    if responses.is_empty() {
        return Err(Error::Unsupported("add at least one response".to_owned()));
    }
    let mut columns: Vec<&str> = responses.iter().map(|r| r.column.as_str()).collect();
    columns.sort_unstable();
    columns.dedup();
    if columns.len() != responses.len() {
        return Err(Error::Unsupported(
            "the response list contains duplicates".to_owned(),
        ));
    }
    if responses
        .iter()
        .any(|response| factors.iter().any(|factor| factor == &response.column))
    {
        return Err(Error::Unsupported(
            "a response cannot also be a factor".to_owned(),
        ));
    }
    for response in responses {
        response.validate()?;
    }
    let models = responses
        .iter()
        .map(|response| rsm::fit(df, &response.column, factors))
        .collect::<Result<Vec<RsmFit>>>()?;

    let objective = |point: &[f64]| -> f64 {
        let coded: Vec<f64> = point.iter().map(|value| value.clamp(-1.0, 1.0)).collect();
        let mut individual = Vec::with_capacity(models.len());
        for (model, response) in models.iter().zip(responses) {
            let Ok(value) = rsm::predict(model, &coded) else {
                return 1.0;
            };
            individual.push(response.desirability(value));
        }
        -combined(responses, &individual)
    };

    let dimension = factors.len();
    let mut starts = vec![vec![0.0; dimension]];
    for index in 0..dimension {
        for sign in [-0.75, 0.75] {
            let mut start = vec![0.0; dimension];
            start[index] = sign;
            starts.push(start);
        }
    }
    starts.push(vec![0.5; dimension]);
    starts.push(vec![-0.5; dimension]);

    let mut best: Option<(f64, Vec<f64>)> = None;
    for start in starts {
        let steps = vec![0.4; dimension];
        let Ok(minimum) = nelder_mead(&start, &steps, 2000, 1e-10, objective) else {
            continue;
        };
        let point: Vec<f64> = minimum
            .arguments
            .iter()
            .map(|value| value.clamp(-1.0, 1.0))
            .collect();
        let value = -objective(&point);
        if best.as_ref().is_none_or(|(current, _)| value > *current) {
            best = Some((value, point));
        }
    }
    let Some((combined_value, optimum)) = best else {
        return Err(Error::NotConverged {
            detail: "desirability optimisation".to_owned(),
        });
    };
    let predicted = models
        .iter()
        .map(|model| rsm::predict(model, &optimum))
        .collect::<Result<Vec<f64>>>()?;
    let individual: Vec<f64> = responses
        .iter()
        .zip(&predicted)
        .map(|(response, value)| response.desirability(*value))
        .collect();
    Ok(DesirabilityFit {
        factors: factors.to_vec(),
        responses: responses.to_vec(),
        optimum,
        predicted,
        individual,
        combined: combined_value,
        models,
    })
}

/// Fits and reports the multi-response optimiser.
///
/// # Errors
///
/// As [`optimize`].
pub fn analyze(
    df: &DataFrame,
    factors: &[String],
    responses: &[Response],
) -> Result<AnalysisOutput> {
    let fit = optimize(df, factors, responses)?;
    let mut result = AnalysisResult::new(
        "Response Optimizer — desirability".to_owned(),
        "Quadratic response surfaces, Derringer–Suich desirability, multi-start search",
    );
    result.n = fit.models[0].regression.n;
    result.excluded = fit
        .models
        .iter()
        .map(|model| model.excluded)
        .max()
        .unwrap_or(0);
    result.statistics = vec![
        ("Combined desirability", fit.combined),
        ("Factors", fit.factors.len() as f64),
        ("Responses", fit.responses.len() as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Factor".to_owned(), "Coded value".to_owned()],
        rows: fit
            .factors
            .iter()
            .zip(&fit.optimum)
            .map(|(factor, value)| vec![factor.clone(), format!("{value:.6}")])
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Response".to_owned(),
            "Goal".to_owned(),
            "Low".to_owned(),
            "Target".to_owned(),
            "High".to_owned(),
            "Predicted".to_owned(),
            "Desirability".to_owned(),
            "Weight".to_owned(),
            "Importance".to_owned(),
        ],
        rows: fit
            .responses
            .iter()
            .zip(&fit.predicted)
            .zip(&fit.individual)
            .map(|((response, predicted), individual)| {
                vec![
                    response.column.clone(),
                    response.goal.name().to_owned(),
                    format!("{:.6}", response.low),
                    if response.goal == Goal::Target {
                        format!("{:.6}", response.target)
                    } else {
                        "—".to_owned()
                    },
                    format!("{:.6}", response.high),
                    format!("{predicted:.6}"),
                    format!("{individual:.6}"),
                    format!("{}", response.weight),
                    format!("{}", response.importance),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Message(format!(
        "Combined desirability {:.6} at the coded point {}",
        fit.combined,
        fit.optimum
            .iter()
            .map(|value| format!("{value:.6}"))
            .collect::<Vec<String>>()
            .join(", ")
    )));
    result.notes = vec![
        (
            "Models".to_owned(),
            "each response is fitted with the full quadratic response-surface model on the same rows"
                .to_owned(),
        ),
        (
            "Search".to_owned(),
            "multi-start Nelder–Mead over the coded region −1…+1".to_owned(),
        ),
        (
            "Combination".to_owned(),
            "weighted geometric mean D = (Π dᵢ^importanceᵢ)^(1/Σ importanceᵢ)".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn response(goal: Goal) -> Response {
        Response {
            column: "y".to_owned(),
            goal,
            low: 0.0,
            target: 5.0,
            high: 10.0,
            weight: 1.0,
            importance: 1.0,
        }
    }

    #[test]
    fn the_desirability_shapes_are_correct() {
        let maximize = response(Goal::Maximize);
        assert_eq!(maximize.desirability(0.0), 0.0);
        assert_eq!(maximize.desirability(5.0), 0.5);
        assert_eq!(maximize.desirability(10.0), 1.0);
        let minimize = response(Goal::Minimize);
        assert_eq!(minimize.desirability(0.0), 1.0);
        assert_eq!(minimize.desirability(10.0), 0.0);
        let target = response(Goal::Target);
        assert_eq!(target.desirability(5.0), 1.0);
        assert_eq!(target.desirability(0.0), 0.0);
        assert_eq!(target.desirability(7.5), 0.5);
    }

    #[test]
    fn the_combined_desirability_is_the_weighted_geometric_mean() {
        let responses = vec![response(Goal::Maximize), response(Goal::Minimize)];
        let value = combined(&responses, &[0.25, 0.81]);
        assert!((value - 0.45).abs() < 1e-12, "{value}");
        assert_eq!(combined(&responses, &[0.0, 1.0]), 0.0);
    }

    fn frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut y1 = Vec::new();
        let mut y2 = Vec::new();
        for first in [-1.0_f64, 0.0, 1.0] {
            for second in [-1.0_f64, 0.0, 1.0] {
                a.push(first);
                b.push(second);
                y1.push(10.0 - (first - 0.3).powi(2) - (second + 0.2).powi(2));
                y2.push(5.0 + (first - 0.3).powi(2) + (second + 0.2).powi(2));
            }
        }
        DataFrame::new(vec![
            Column::new_f64("A", a),
            Column::new_f64("B", b),
            Column::new_f64("y1", y1),
            Column::new_f64("y2", y2),
        ])
        .expect("frame")
    }

    #[test]
    fn the_optimiser_finds_the_common_optimum() {
        let responses = vec![
            Response {
                column: "y1".to_owned(),
                goal: Goal::Maximize,
                low: 5.0,
                target: 10.0,
                high: 10.0,
                weight: 1.0,
                importance: 1.0,
            },
            Response {
                column: "y2".to_owned(),
                goal: Goal::Minimize,
                low: 5.0,
                target: 5.0,
                high: 10.0,
                weight: 1.0,
                importance: 1.0,
            },
        ];
        let fit =
            optimize(&frame(), &["A".to_owned(), "B".to_owned()], &responses).expect("optimiser");
        assert!((fit.optimum[0] - 0.3).abs() < 1e-3, "{:?}", fit.optimum);
        assert!((fit.optimum[1] + 0.2).abs() < 1e-3, "{:?}", fit.optimum);
        assert!(fit.combined > 0.99, "{}", fit.combined);
    }

    #[test]
    fn invalid_responses_are_rejected() {
        let df = frame();
        assert!(optimize(&df, &["A".to_owned(), "B".to_owned()], &[]).is_err());
        let mut bad = response(Goal::Maximize);
        bad.low = 10.0;
        bad.high = 0.0;
        assert!(bad.validate().is_err());
        let mut bad = response(Goal::Target);
        bad.target = 20.0;
        assert!(bad.validate().is_err());
        let duplicate = vec![response(Goal::Maximize), response(Goal::Minimize)];
        assert!(optimize(&df, &["A".to_owned(), "B".to_owned()], &duplicate).is_err());
        let mut as_factor = response(Goal::Maximize);
        as_factor.column = "A".to_owned();
        assert!(optimize(&df, &["A".to_owned(), "B".to_owned()], &[as_factor]).is_err());
    }
}
