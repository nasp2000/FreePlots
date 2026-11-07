//! Independent reference values for the proportional-odds model.
//!
//! The values in `tests/reference/ordinal.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::ordinal;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/ordinal.json");
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

fn frame(case: &Value) -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("x1", numbers(&case["x1"])),
        Column::new_f64("x2", numbers(&case["x2"])),
        Column::new_f64("y", numbers(&case["y"])),
    ])
    .expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs reference {expected}"
    );
}

#[test]
fn ordinal_logistic_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model =
        ordinal::fit(&frame(&r), "y", &["x1".to_owned(), "x2".to_owned()]).expect("ordinal fit");

    for (index, expected) in numbers(&r["thresholds"]).into_iter().enumerate() {
        close(model.thresholds[index], expected, tolerance, "threshold");
    }
    for (index, expected) in numbers(&r["coefficients"]).into_iter().enumerate() {
        close(
            model.coefficients[index],
            expected,
            tolerance,
            "coefficient",
        );
        close(
            model.standard_errors[index],
            numbers(&r["standard_errors"])[index],
            tolerance,
            "standard error",
        );
        close(model.z[index], numbers(&r["z"])[index], tolerance, "z");
        close(
            model.p_values[index],
            numbers(&r["p_values"])[index],
            tolerance,
            "p",
        );
    }
    close(
        model.log_likelihood,
        r["log_likelihood"].as_f64().expect("llf"),
        tolerance,
        "log-likelihood",
    );
    close(model.aic, r["aic"].as_f64().expect("aic"), tolerance, "AIC");
    assert!(model.thresholds[0] < model.thresholds[1]);
    assert_eq!(model.n, 60);
}

#[test]
fn a_perfectly_separated_sample_does_not_produce_a_finite_fit() {
    let x: Vec<f64> = (0..60).map(|index| index as f64 / 10.0).collect();
    let y: Vec<f64> = x
        .iter()
        .map(|value| {
            if *value < 2.0 {
                1.0
            } else if *value < 4.0 {
                2.0
            } else {
                3.0
            }
        })
        .collect();
    let df = DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
    // The likelihood is unbounded (the slope goes to infinity): the fit must
    // fail rather than return an arbitrary finite point.
    assert!(ordinal::fit(&df, "y", &["x".to_owned()]).is_err());
}

#[test]
fn two_categories_are_rejected() {
    let df = DataFrame::new(vec![
        Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0]),
        Column::new_f64("y", vec![1.0, 1.0, 2.0, 2.0]),
    ])
    .expect("frame");
    assert!(ordinal::fit(&df, "y", &["x".to_owned()]).is_err());
}
