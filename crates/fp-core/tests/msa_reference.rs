// SPDX-License-Identifier: MIT
//! Independent reference values for the Type 1 and linearity studies.
//!
//! The values in `tests/reference/msa.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::{gage_type1, linearity};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/msa.json");
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

#[test]
fn type_one_matches_the_oracle() {
    let r = reference();
    let case = &r["type1"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = DataFrame::new(vec![Column::new_f64(
        "measurement",
        numbers(&case["values"]),
    )])
    .expect("frame");
    let model = gage_type1::fit(
        &df,
        "measurement",
        case["reference"].as_f64().expect("reference"),
        Some(case["lsl"].as_f64().expect("lsl")),
        Some(case["usl"].as_f64().expect("usl")),
    )
    .expect("type 1");
    assert_eq!(model.n, 20);
    for (actual, key) in [
        (model.mean, "mean"),
        (model.sd, "sd"),
        (model.bias, "bias"),
        (model.se_bias, "se_bias"),
        (model.t, "t"),
        (model.p_value, "p_value"),
        (model.repeatability, "repeatability"),
        (
            model.percent_tolerance.expect("percent tolerance"),
            "percent_tolerance",
        ),
        (model.percent_bias.expect("percent bias"), "percent_bias"),
    ] {
        close(actual, case[key].as_f64().expect("value"), tolerance, key);
    }

    // Without specification limits the percentages are not reported.
    let model = gage_type1::fit(
        &df,
        "measurement",
        case["reference"].as_f64().expect("reference"),
        None,
        None,
    )
    .expect("type 1 without limits");
    assert!(model.percent_tolerance.is_none());
    assert!(model.percent_bias.is_none());
}

#[test]
fn linearity_matches_the_oracle() {
    let r = reference();
    let case = &r["linearity"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = DataFrame::new(vec![
        Column::new_f64("measurement", numbers(&case["measurements"])),
        Column::new_f64("reference", numbers(&case["references"])),
    ])
    .expect("frame");
    let model = linearity::fit(&df, "measurement", "reference").expect("linearity");
    assert_eq!(model.n, 15);
    assert_eq!(model.references.len(), 5);
    assert_eq!(model.counts, vec![3, 3, 3, 3, 3]);
    for (actual, expected) in model
        .references
        .iter()
        .zip(numbers(&case["part_references"]))
    {
        close(*actual, expected, tolerance, "reference");
    }
    for (actual, expected) in model.means.iter().zip(numbers(&case["means"])) {
        close(*actual, expected, tolerance, "mean");
    }
    for (actual, expected) in model.biases.iter().zip(numbers(&case["biases"])) {
        close(*actual, expected, tolerance, "bias");
    }
    for (actual, key) in [
        (model.mean_bias, "mean_bias"),
        (model.bias_se, "bias_se"),
        (model.bias_t, "bias_t"),
        (model.bias_p, "bias_p"),
        (model.slope, "slope"),
        (model.intercept, "intercept"),
        (model.slope_se, "slope_se"),
        (model.slope_t, "slope_t"),
        (model.slope_p, "slope_p"),
        (model.r_squared, "r_squared"),
        (model.process_variation, "process_variation"),
        (model.linearity, "linearity"),
        (model.percent_linearity, "percent_linearity"),
        (model.percent_bias, "percent_bias"),
    ] {
        close(actual, case[key].as_f64().expect("value"), tolerance, key);
    }
}

#[test]
fn invalid_inputs_are_rejected() {
    let r = reference();
    let case = &r["type1"];
    let constant =
        DataFrame::new(vec![Column::new_f64("measurement", vec![10.0; 10])]).expect("frame");
    assert!(gage_type1::fit(&constant, "measurement", 10.0, None, None).is_err());
    let values = DataFrame::new(vec![Column::new_f64(
        "measurement",
        numbers(&case["values"]),
    )])
    .expect("frame");
    assert!(gage_type1::fit(&values, "measurement", 10.0, Some(11.0), Some(9.0)).is_err());

    let flat = DataFrame::new(vec![
        Column::new_f64("measurement", vec![1.0, 2.0, 3.0]),
        Column::new_f64("reference", vec![5.0, 5.0, 5.0]),
    ])
    .expect("frame");
    assert!(linearity::fit(&flat, "measurement", "reference").is_err());
}
