//! Independent values for forward and backward stepwise selection.
//!
//! The values in `tests/reference/stepwise.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::stepwise::{self, StepMethod, StepwiseOptions};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/stepwise.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .expect("number list")
        .iter()
        .map(|value| value.as_f64().expect("number"))
        .collect()
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs oracle {expected}"
    );
}

fn frame(r: &Value) -> DataFrame {
    let data = &r["data"];
    let mut columns = Vec::new();
    for name in ["y", "x1", "x2", "x3"] {
        columns.push(Column::new_f64(name, numbers(&data[name])));
    }
    DataFrame::new(columns).expect("frame")
}

fn check(method: StepMethod, case: &Value, r: &Value) {
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let candidates: Vec<String> = r["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .map(|value| value.as_str().expect("candidate").to_owned())
        .collect();
    let alpha = r["alpha"].as_f64().expect("alpha");
    let options = StepwiseOptions {
        method,
        alpha_enter: alpha,
        alpha_remove: alpha,
        ..StepwiseOptions::default()
    };
    let fit = stepwise::fit(&frame(r), "y", &candidates, options).expect("stepwise");
    let selected: Vec<String> = case["selected"]
        .as_array()
        .expect("selected")
        .iter()
        .map(|value| value.as_str().expect("predictor").to_owned())
        .collect();
    assert_eq!(fit.selected, selected);
    let steps = case["steps"].as_array().expect("steps");
    assert_eq!(fit.steps.len(), steps.len());
    for (actual, expected) in fit.steps.iter().zip(steps) {
        assert_eq!(
            actual.predictor,
            expected["predictor"].as_str().expect("predictor")
        );
        let action = match actual.action {
            stepwise::StepAction::Added => "added",
            stepwise::StepAction::Removed => "removed",
        };
        assert_eq!(action, expected["action"].as_str().expect("action"));
        close(
            actual.p_value,
            expected["p_value"].as_f64().expect("p"),
            tolerance,
            "partial p",
        );
        close(
            actual.r_squared,
            expected["r_squared"].as_f64().expect("R²"),
            tolerance,
            "R²",
        );
        close(
            actual.adjusted_r_squared,
            expected["adjusted_r_squared"]
                .as_f64()
                .expect("adjusted R²"),
            tolerance,
            "adjusted R²",
        );
        close(
            actual.aic,
            expected["aic"].as_f64().expect("AIC"),
            tolerance,
            "AIC",
        );
        close(
            actual.bic,
            expected["bic"].as_f64().expect("BIC"),
            tolerance,
            "BIC",
        );
    }
    let coefficients = numbers(&case["coefficients"]);
    assert_eq!(fit.model.coefficients.len(), coefficients.len());
    for (actual, expected) in fit.model.coefficients.iter().zip(coefficients) {
        close(*actual, expected, tolerance, "coefficient");
    }
    for (actual, expected) in fit
        .model
        .standard_errors
        .iter()
        .zip(numbers(&case["standard_errors"]))
    {
        close(*actual, expected, tolerance, "standard error");
    }
    for (actual, expected) in fit.model.p_values.iter().zip(numbers(&case["p_values"])) {
        close(*actual, expected, tolerance, "p");
    }
    close(
        fit.model.r_squared,
        case["r_squared"].as_f64().expect("R²"),
        tolerance,
        "final R²",
    );
}

#[test]
fn forward_selection_matches_the_partial_f_tests() {
    let r = reference();
    check(StepMethod::Forward, &r["forward"], &r);
}

#[test]
fn backward_elimination_matches_the_partial_f_tests() {
    let r = reference();
    check(StepMethod::Backward, &r["backward"], &r);
}

#[test]
fn invalid_candidate_lists_are_rejected() {
    let r = reference();
    let df = frame(&r);
    let options = StepwiseOptions::default();
    assert!(stepwise::fit(&df, "y", &[], options).is_err());
    assert!(stepwise::fit(&df, "y", &["y".to_owned()], options).is_err());
    assert!(stepwise::fit(&df, "y", &["x1".to_owned(), "x1".to_owned()], options).is_err());
    assert!(stepwise::fit(&df, "y", &["missing".to_owned()], options).is_err());
    assert!(
        stepwise::fit(
            &df,
            "y",
            &["x1".to_owned()],
            StepwiseOptions {
                alpha_enter: 1.5,
                ..options
            }
        )
        .is_err()
    );
}
