//! Stepwise selection for ordinary least squares regression.
//!
//! Forward selection, backward elimination and the bidirectional stepwise
//! search over numeric predictors. Every candidate model is fitted with the
//! same OLS engine as [`crate::regression`], and all models share the rows
//! that are complete for the response and for every candidate, so the
//! information criteria and the partial F tests are comparable across steps.
//!
//! The p-value criterion enters a predictor when the partial F test against
//! the current model has `p ≤ α_enter` and removes it when `p > α_remove`;
//! the AIC/BIC criterion keeps the move that lowers
//! `n·ln(SSE/n) + penalty·(k+1)`.

use crate::data::{Column, ColumnData, DType, DataFrame};
use crate::error::{Error, Result};
use crate::regression::{self, RegressionFit};
use crate::report::{Item, standard_warnings};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// Direction of the stepwise search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMethod {
    /// Start from the intercept and add predictors.
    Forward,
    /// Start from every candidate and remove predictors.
    Backward,
    /// Alternate removal and addition.
    Stepwise,
}

impl StepMethod {
    /// Parses the CLI/UI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "forward" => Some(Self::Forward),
            "backward" => Some(Self::Backward),
            "stepwise" | "both" => Some(Self::Stepwise),
            _ => None,
        }
    }

    /// Human-readable name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward selection",
            Self::Backward => "backward elimination",
            Self::Stepwise => "stepwise selection",
        }
    }
}

/// Rule that decides whether a move improves the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepCriterion {
    /// Partial F tests with an entry and a removal alpha.
    PValue,
    /// Akaike information criterion.
    Aic,
    /// Bayesian information criterion.
    Bic,
}

impl StepCriterion {
    /// Parses the CLI/UI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "p" | "pvalue" | "p-value" | "alpha" => Some(Self::PValue),
            "aic" => Some(Self::Aic),
            "bic" | "sbc" => Some(Self::Bic),
            _ => None,
        }
    }

    /// Human-readable name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PValue => "p-value",
            Self::Aic => "AIC",
            Self::Bic => "BIC",
        }
    }
}

/// Kind of move recorded in the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepAction {
    /// A predictor entered the model.
    Added,
    /// A predictor left the model.
    Removed,
}

impl StepAction {
    /// Human-readable name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Added => "add",
            Self::Removed => "remove",
        }
    }
}

/// Options of the stepwise search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepwiseOptions {
    /// Search direction.
    pub method: StepMethod,
    /// Rule that accepts a move.
    pub criterion: StepCriterion,
    /// Entry alpha of the p-value criterion.
    pub alpha_enter: f64,
    /// Removal alpha of the p-value criterion.
    pub alpha_remove: f64,
    /// Maximum number of accepted moves.
    pub max_steps: usize,
    /// Confidence level of the final coefficient intervals.
    pub confidence: f64,
}

impl Default for StepwiseOptions {
    fn default() -> Self {
        Self {
            method: StepMethod::Stepwise,
            criterion: StepCriterion::PValue,
            alpha_enter: 0.15,
            alpha_remove: 0.15,
            max_steps: 50,
            confidence: 0.95,
        }
    }
}

/// One accepted move of the search.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRecord {
    /// Step number, starting at one.
    pub step: usize,
    /// Whether the predictor entered or left.
    pub action: StepAction,
    /// Predictor name.
    pub predictor: String,
    /// Partial F p-value of the move.
    pub p_value: f64,
    /// Coefficient of determination after the move.
    pub r_squared: f64,
    /// Adjusted coefficient of determination after the move.
    pub adjusted_r_squared: f64,
    /// AIC of the model after the move.
    pub aic: f64,
    /// BIC of the model after the move.
    pub bic: f64,
}

/// The selected model and the path that reached it.
#[derive(Debug, Clone, PartialEq)]
pub struct StepwiseFit {
    /// Options used.
    pub options: StepwiseOptions,
    /// Predictors in the final model, in selection order.
    pub selected: Vec<String>,
    /// Accepted moves in order.
    pub steps: Vec<StepRecord>,
    /// Final ordinary least squares fit.
    pub model: RegressionFit,
    /// Number of candidate predictors offered.
    pub candidates: usize,
    /// Candidate columns skipped because a model with them could not be fitted.
    pub skipped: Vec<String>,
    /// Rows used after the listwise deletion of the candidates.
    pub n: usize,
    /// Rows removed listwise.
    pub excluded: usize,
}

/// A candidate model reduced to what the search compares.
#[derive(Debug, Clone)]
struct CandidateModel {
    sse: f64,
    /// Parameters including the intercept.
    p: usize,
}

fn numeric_cell(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn validate(options: &StepwiseOptions) -> Result<()> {
    if !options.confidence.is_finite() || !(0.5..1.0).contains(&options.confidence) {
        return Err(Error::InvalidParameter {
            name: "confidence level",
            value: options.confidence,
        });
    }
    if options.max_steps == 0 || options.max_steps > 1000 {
        return Err(Error::InvalidParameter {
            name: "maximum number of steps",
            value: options.max_steps as f64,
        });
    }
    if options.criterion == StepCriterion::PValue {
        for (name, alpha) in [
            ("entry alpha", options.alpha_enter),
            ("removal alpha", options.alpha_remove),
        ] {
            if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
                return Err(Error::InvalidParameter { name, value: alpha });
            }
        }
    }
    Ok(())
}

/// Information criterion of a model with `p` parameters and `n` rows.
fn information(sse: f64, n: usize, p: usize, criterion: StepCriterion) -> f64 {
    let penalty = match criterion {
        StepCriterion::Aic | StepCriterion::PValue => 2.0,
        StepCriterion::Bic => (n as f64).ln(),
    };
    n as f64 * (sse / n as f64).ln() + penalty * p as f64
}

/// p-value of the partial F test that compares a reduced with a full model.
fn partial_f_p_value(reduced: &CandidateModel, full: &CandidateModel, n: usize) -> f64 {
    if reduced.sse <= 0.0 {
        return 1.0;
    }
    if full.p <= reduced.p || full.sse <= 0.0 || n <= full.p {
        return if full.sse <= 0.0 { 0.0 } else { 1.0 };
    }
    let q = (full.p - reduced.p) as f64;
    let df_full = (n - full.p) as f64;
    let f = ((reduced.sse - full.sse).max(0.0) / q) / (full.sse / df_full);
    if !f.is_finite() {
        return 0.0;
    }
    (1.0 - f_cdf(f, q, df_full)).clamp(0.0, 1.0)
}

fn evaluate(
    clean: &DataFrame,
    response: &str,
    set: &[String],
    sst: f64,
    confidence: f64,
) -> Result<CandidateModel> {
    if set.is_empty() {
        return Ok(CandidateModel { sse: sst, p: 1 });
    }
    let fit = regression::fit(clean, response, set, confidence)?;
    Ok(CandidateModel {
        sse: fit.residual_ss,
        p: fit.names.len(),
    })
}

/// Runs the stepwise search.
pub fn fit(
    df: &DataFrame,
    response: &str,
    candidates: &[String],
    options: StepwiseOptions,
) -> Result<StepwiseFit> {
    validate(&options)?;
    if candidates.is_empty() {
        return Err(Error::Unsupported(
            "stepwise regression needs at least one candidate predictor".to_owned(),
        ));
    }
    if candidates.iter().any(|name| name == response) {
        return Err(Error::Unsupported(
            "the response cannot also be a candidate".to_owned(),
        ));
    }
    let mut unique = candidates.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != candidates.len() {
        return Err(Error::Unsupported(
            "the candidate list contains duplicate columns".to_owned(),
        ));
    }

    let response_column = df.column(response)?;
    if !matches!(response_column.dtype(), DType::Float | DType::Integer) {
        return Err(Error::Csv(format!(
            "column {response} is not numeric and cannot be a regression response"
        )));
    }
    let mut predictor_columns = Vec::with_capacity(candidates.len());
    for name in candidates {
        let column = df.column(name)?;
        if !matches!(column.dtype(), DType::Float | DType::Integer) {
            return Err(Error::Unsupported(format!(
                "stepwise selection needs numeric predictors; {name} is not numeric"
            )));
        }
        predictor_columns.push(column);
    }

    // A single sample shared by every candidate model.
    let mut rows: Vec<usize> = Vec::new();
    for row in 0..df.n_rows() {
        if numeric_cell(response_column, row).is_some()
            && predictor_columns
                .iter()
                .all(|column| numeric_cell(column, row).is_some())
        {
            rows.push(row);
        }
    }
    let n = rows.len();
    if n <= candidates.len() + 1 {
        return Err(Error::SampleTooSmall {
            n,
            min: candidates.len() + 2,
        });
    }

    let mut columns = Vec::with_capacity(candidates.len() + 1);
    let response_values: Vec<f64> = rows
        .iter()
        .filter_map(|&row| numeric_cell(response_column, row))
        .collect();
    let mean = response_values.iter().sum::<f64>() / n as f64;
    let sst = response_values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>();
    if !sst.is_finite() || sst <= 0.0 {
        return Err(Error::ConstantSample);
    }
    columns.push(Column::new_f64(response, response_values));
    for (name, column) in candidates.iter().zip(&predictor_columns) {
        let values: Vec<f64> = rows
            .iter()
            .filter_map(|&row| numeric_cell(column, row))
            .collect();
        columns.push(Column::new_f64(name.clone(), values));
    }
    let clean = DataFrame::new(columns)?;

    let mut set: Vec<String> = if options.method == StepMethod::Backward {
        candidates.to_vec()
    } else {
        Vec::new()
    };
    let mut current = evaluate(&clean, response, &set, sst, options.confidence)?;
    let mut steps: Vec<StepRecord> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    while steps.len() < options.max_steps {
        let mut best_remove: Option<(String, CandidateModel, f64)> = None;
        if options.method != StepMethod::Forward {
            for name in &set {
                let trial: Vec<String> =
                    set.iter().filter(|value| *value != name).cloned().collect();
                let Ok(model) = evaluate(&clean, response, &trial, sst, options.confidence) else {
                    continue;
                };
                let p = partial_f_p_value(&model, &current, n);
                let better = match &best_remove {
                    None => true,
                    Some((_, _, best)) => p > *best,
                };
                if better {
                    best_remove = Some((name.clone(), model, p));
                }
            }
        }
        let mut best_add: Option<(String, CandidateModel, f64)> = None;
        if options.method != StepMethod::Backward {
            for name in candidates {
                if set.contains(name) {
                    continue;
                }
                let mut trial = set.clone();
                trial.push(name.clone());
                let Ok(model) = evaluate(&clean, response, &trial, sst, options.confidence) else {
                    if !skipped.contains(name) {
                        skipped.push(name.clone());
                    }
                    continue;
                };
                let p = partial_f_p_value(&current, &model, n);
                let better = match &best_add {
                    None => true,
                    Some((_, _, best)) => p < *best,
                };
                if better {
                    best_add = Some((name.clone(), model, p));
                }
            }
        }

        let chosen: Option<(StepAction, String, CandidateModel, f64)> = match options.criterion {
            StepCriterion::PValue => {
                if let Some((name, model, p)) =
                    best_remove.filter(|(_, _, p)| *p > options.alpha_remove)
                {
                    Some((StepAction::Removed, name, model, p))
                } else {
                    best_add
                        .filter(|(_, _, p)| *p <= options.alpha_enter)
                        .map(|(name, model, p)| (StepAction::Added, name, model, p))
                }
            }
            StepCriterion::Aic | StepCriterion::Bic => {
                let current_score = information(current.sse, n, current.p, options.criterion);
                let mut best: Option<(StepAction, String, CandidateModel, f64, f64)> = None;
                for (action, candidate) in best_remove
                    .map(|(name, model, p)| (StepAction::Removed, (name, model, p)))
                    .into_iter()
                    .chain(best_add.map(|(name, model, p)| (StepAction::Added, (name, model, p))))
                {
                    let (name, model, p) = candidate;
                    let score = information(model.sse, n, model.p, options.criterion);
                    if score >= current_score - 1e-9 {
                        continue;
                    }
                    let better = match &best {
                        None => true,
                        Some((_, _, _, _, best_score)) => score < *best_score,
                    };
                    if better {
                        best = Some((action, name, model, p, score));
                    }
                }
                best.map(|(action, name, model, p, _)| (action, name, model, p))
            }
        };

        let Some((action, name, model, p)) = chosen else {
            break;
        };
        match action {
            StepAction::Added => set.push(name.clone()),
            StepAction::Removed => set.retain(|value| *value != name),
        }
        current = model;
        let r_squared = (1.0 - current.sse / sst).clamp(0.0, 1.0);
        let df_residual = (n - current.p) as f64;
        let adjusted_r_squared = 1.0 - (1.0 - r_squared) * (n as f64 - 1.0) / df_residual;
        steps.push(StepRecord {
            step: steps.len() + 1,
            action,
            predictor: name,
            p_value: p,
            r_squared,
            adjusted_r_squared,
            aic: information(current.sse, n, current.p, StepCriterion::Aic),
            bic: information(current.sse, n, current.p, StepCriterion::Bic),
        });
    }

    if set.is_empty() {
        return Err(Error::Unsupported(
            "no candidate met the entry criterion; the final model would be the intercept only"
                .to_owned(),
        ));
    }

    let model = regression::fit(&clean, response, &set, options.confidence)?;
    Ok(StepwiseFit {
        options,
        selected: set,
        steps,
        model,
        candidates: candidates.len(),
        skipped,
        n,
        excluded: df.n_rows() - n,
    })
}

fn p_text(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        "—".to_owned()
    }
}

/// Runs the search and builds the Session report.
pub fn analyze(
    df: &DataFrame,
    response: &str,
    candidates: &[String],
    options: StepwiseOptions,
) -> Result<AnalysisOutput> {
    let fit = fit(df, response, candidates, options)?;
    let model = &fit.model;
    let mut result = regression::report(response, model);
    result.title = format!("Stepwise Regression — response {response}");
    result.method = match options.criterion {
        StepCriterion::PValue => format!(
            "{}, p-value criterion (enter α = {}, remove α = {}), {:.1}% confidence",
            options.method.name(),
            options.alpha_enter,
            options.alpha_remove,
            options.confidence * 100.0
        ),
        StepCriterion::Aic | StepCriterion::Bic => format!(
            "{}, {} criterion, {:.1}% confidence",
            options.method.name(),
            options.criterion.name(),
            options.confidence * 100.0
        ),
    };
    result.n = fit.n;
    result.excluded = fit.excluded;
    result.statistics = vec![
        ("Candidates", fit.candidates as f64),
        ("Selected", fit.selected.len() as f64),
        ("Steps", fit.steps.len() as f64),
        ("R²", model.r_squared),
        ("Adjusted R²", model.adjusted_r_squared),
        ("Residual standard error", model.s),
        ("F", model.f),
        ("F p-value", model.f_p_value),
        (
            "AIC",
            information(
                model.residual_ss,
                fit.n,
                model.names.len(),
                StepCriterion::Aic,
            ),
        ),
        (
            "BIC",
            information(
                model.residual_ss,
                fit.n,
                model.names.len(),
                StepCriterion::Bic,
            ),
        ),
    ];
    if !fit.steps.is_empty() {
        result.extra.insert(
            0,
            Item::Table {
                headers: vec![
                    "Step".to_owned(),
                    "Action".to_owned(),
                    "Predictor".to_owned(),
                    "P".to_owned(),
                    "R²".to_owned(),
                    "Adj R²".to_owned(),
                    "AIC".to_owned(),
                    "BIC".to_owned(),
                ],
                rows: fit
                    .steps
                    .iter()
                    .map(|record| {
                        vec![
                            record.step.to_string(),
                            record.action.name().to_owned(),
                            record.predictor.clone(),
                            p_text(record.p_value),
                            format!("{:.4}", record.r_squared),
                            format!("{:.4}", record.adjusted_r_squared),
                            format!("{:.4}", record.aic),
                            format!("{:.4}", record.bic),
                        ]
                    })
                    .collect(),
            },
        );
    }
    if !fit.skipped.is_empty() {
        result.extra.push(Item::Message(format!(
            "Predictors skipped because a model with them could not be fitted: {}",
            fit.skipped.join(", ")
        )));
    }
    result.notes.insert(
        0,
        (
            "Selection".to_owned(),
            format!(
                "{} over {} numeric candidate(s); every candidate model uses the same rows, so the partial F tests and the information criteria are comparable",
                options.method.name(),
                fit.candidates
            ),
        ),
    );
    result.notes.push((
        "Criterion".to_owned(),
        match options.criterion {
            StepCriterion::PValue => format!(
                "a predictor enters when the partial F test has p ≤ {} and leaves when p > {}; the partial F compares the model with and without the predictor",
                options.alpha_enter, options.alpha_remove
            ),
            StepCriterion::Aic | StepCriterion::Bic => format!(
                "{} = n·ln(SSE/n) + {}·(k+1); a move is accepted only when it lowers the criterion",
                options.criterion.name(),
                if options.criterion == StepCriterion::Aic {
                    "2".to_owned()
                } else {
                    "ln(n)".to_owned()
                }
            ),
        },
    ));
    result.notes.push((
        "Selection caveat".to_owned(),
        "the coefficient tests of the final model condition on the selected model and ignore the search; they are optimistic and should be read as descriptive".to_owned(),
    ));
    result.notes.push((
        "Selection sample".to_owned(),
        "a row is used only when the response and every candidate predictor are finite; the same sample enters every candidate model".to_owned(),
    ));
    result.warnings = standard_warnings(fit.n, fit.excluded, 0);
    result.warnings.push(
        "stepwise selection follows standard forward/backward conventions and has not been verified against captured output"
            .to_owned(),
    );
    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(regression::residual_plot(response, model)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> DataFrame {
        // x1 drives the response; x2 and x3 are orthogonal to x1 and to the
        // residual, so they never improve the fit.
        let x1: Vec<f64> = (0..12).map(|value| value as f64).collect();
        let y = vec![
            1.410456, 4.198339, 5.105444, 5.594605, 8.019128, 11.014304, 13.465211, 14.88367,
            16.664885, 18.692888, 21.334876, 23.616195,
        ];
        let x2 = vec![
            0.769231, -1.188811, 0.853147, -1.104895, 0.937063, -1.020979, 1.020979, -0.937063,
            1.104895, -0.853147, 1.188811, -0.769231,
        ];
        let x3 = vec![
            -0.571429, -0.571429, 1.257143, 1.257143, -0.914286, -0.914286, 0.914286, 0.914286,
            -1.257143, -1.257143, 0.571429, 0.571429,
        ];
        DataFrame::new(vec![
            Column::new_f64("y", y),
            Column::new_f64("x1", x1),
            Column::new_f64("x2", x2),
            Column::new_f64("x3", x3),
        ])
        .expect("frame")
    }

    fn candidates() -> Vec<String> {
        vec!["x1".to_owned(), "x2".to_owned(), "x3".to_owned()]
    }

    #[test]
    fn forward_selection_keeps_the_linked_predictor() {
        let options = StepwiseOptions {
            method: StepMethod::Forward,
            alpha_enter: 0.05,
            ..StepwiseOptions::default()
        };
        let fit = fit(&data(), "y", &candidates(), options).expect("fit");
        assert_eq!(fit.selected, vec!["x1".to_owned()]);
        assert_eq!(fit.steps.len(), 1);
        assert_eq!(fit.steps[0].action, StepAction::Added);
        assert!(fit.steps[0].p_value < 0.05);
        assert!((fit.model.coefficients[0] - 1.0).abs() < 1e-4);
        assert!((fit.model.coefficients[1] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn backward_elimination_drops_the_unrelated_predictors() {
        let options = StepwiseOptions {
            method: StepMethod::Backward,
            alpha_remove: 0.05,
            ..StepwiseOptions::default()
        };
        let fit = fit(&data(), "y", &candidates(), options).expect("fit");
        assert_eq!(fit.selected, vec!["x1".to_owned()]);
        assert_eq!(fit.steps.len(), 2);
        assert!(
            fit.steps
                .iter()
                .all(|step| step.action == StepAction::Removed)
        );
    }

    #[test]
    fn bic_criterion_selects_the_linked_predictor() {
        let options = StepwiseOptions {
            criterion: StepCriterion::Bic,
            ..StepwiseOptions::default()
        };
        let fit = fit(&data(), "y", &candidates(), options).expect("fit");
        assert_eq!(fit.selected, vec!["x1".to_owned()]);
    }

    #[test]
    fn categorical_candidates_are_rejected() {
        let frame = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "label",
                vec![
                    "a".to_owned(),
                    "b".to_owned(),
                    "a".to_owned(),
                    "b".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        let error = fit(
            &frame,
            "y",
            &["label".to_owned()],
            StepwiseOptions::default(),
        )
        .expect_err("categorical candidate");
        assert!(error.to_string().contains("numeric"));
    }

    #[test]
    fn duplicate_candidates_are_rejected() {
        let error = fit(
            &data(),
            "y",
            &["x1".to_owned(), "x1".to_owned()],
            StepwiseOptions::default(),
        )
        .expect_err("duplicates");
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn missing_rows_are_removed_once_for_every_model() {
        let mut frame = data();
        frame.set_missing(0, 2).expect("missing response");
        frame.set_missing(1, 5).expect("missing predictor");
        let fit = fit(
            &frame,
            "y",
            &["x1".to_owned()],
            StepwiseOptions {
                method: StepMethod::Forward,
                alpha_enter: 0.05,
                ..StepwiseOptions::default()
            },
        )
        .expect("fit");
        assert_eq!(fit.n, 10);
        assert_eq!(fit.excluded, 2);
    }
}
