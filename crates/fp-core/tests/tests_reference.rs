// SPDX-License-Identifier: MIT
//! Independent reference values for the t and variance tests .
//!
//! The values in `tests/reference/tests.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::hypothesis::{self, Alternative};
use fp_core::variance;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/tests.json");
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

fn frame_one(case: &Value) -> DataFrame {
    DataFrame::new(vec![Column::new_f64("x", numbers(&case["data"]))]).expect("frame")
}

fn frame_two(case: &Value) -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("a", numbers(&case["first"])),
        Column::new_f64("b", numbers(&case["second"])),
    ])
    .expect("frame")
}

fn number(text: &str, label: &str) -> f64 {
    text.lines()
        .find_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            (tokens.first() == Some(&label) && tokens.len() > 1)
                .then(|| tokens[1].parse().ok())
                .flatten()
        })
        .unwrap_or_else(|| panic!("no {label} in:\n{text}"))
}

/// The two rows labelled `CI lower` and `CI upper`, in that order.
fn bounds(text: &str) -> (f64, f64) {
    let values: Vec<f64> = text
        .lines()
        .filter(|line| line.trim_start().starts_with("CI "))
        .filter_map(|line| line.split_whitespace().nth(2)?.parse().ok())
        .collect();
    assert_eq!(values.len(), 2, "{text}");
    (values[0], values[1])
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs reference {expected}"
    );
}

fn check_bounds(text: &str, expected: &Value, tolerance: f64) {
    let ci = numbers(expected);
    let (low, high) = bounds(text);
    close(low, ci[0], tolerance, "CI low");
    close(high, ci[1], tolerance, "CI high");
}

#[test]
fn one_sample_t_matches_reference() {
    let r = reference();
    let case = &r["one_sample"];
    let tolerance = 1e-6;
    let result = hypothesis::one_sample(
        &frame_one(case),
        "x",
        case["null_mean"].as_f64().expect("mean"),
        Alternative::TwoSided,
        0.95,
    )
    .expect("one-sample t");
    let text = result.to_session_text();
    close(
        number(&text, "t"),
        case["t"].as_f64().expect("t"),
        tolerance,
        "t",
    );
    close(
        number(&text, "p-value"),
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    check_bounds(&text, &case["ci"], tolerance);
}

#[test]
fn pooled_two_sample_t_matches_reference() {
    let r = reference();
    let case = &r["pooled"];
    let tolerance = 1e-6;
    let result = hypothesis::two_sample(
        &frame_two(case),
        "a",
        "b",
        true,
        Alternative::TwoSided,
        0.95,
    )
    .expect("pooled t");
    let text = result.to_session_text();
    close(
        number(&text, "t"),
        case["t"].as_f64().expect("t"),
        tolerance,
        "t",
    );
    close(
        number(&text, "df"),
        case["degrees_of_freedom"].as_f64().expect("df"),
        tolerance,
        "df",
    );
    close(
        number(&text, "p-value"),
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    check_bounds(&text, &case["ci"], tolerance);
}

#[test]
fn paired_t_matches_reference() {
    let r = reference();
    let case = &r["paired"];
    let tolerance = 1e-6;
    let result = hypothesis::paired(&frame_two(case), "a", "b", Alternative::TwoSided, 0.95)
        .expect("paired t");
    let text = result.to_session_text();
    close(
        number(&text, "t"),
        case["t"].as_f64().expect("t"),
        tolerance,
        "t",
    );
    close(
        number(&text, "df"),
        case["degrees_of_freedom"].as_f64().expect("df"),
        tolerance,
        "df",
    );
    close(
        number(&text, "p-value"),
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    check_bounds(&text, &case["ci"], tolerance);
}

#[test]
fn one_sample_variance_matches_reference() {
    let r = reference();
    let case = &r["variance_one"];
    let tolerance = 1e-6;
    let result = variance::one_sample(
        &frame_one(case),
        "x",
        case["null_variance"].as_f64().expect("variance"),
        Alternative::TwoSided,
        0.95,
    )
    .expect("one variance");
    let text = result.to_session_text();
    close(
        number(&text, "Chi-square"),
        case["chi_square"].as_f64().expect("chi-square"),
        tolerance,
        "chi-square",
    );
    close(
        number(&text, "p-value"),
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    check_bounds(&text, &case["ci"], tolerance);
}

#[test]
fn two_sample_variance_matches_reference() {
    let r = reference();
    let case = &r["variance_two"];
    let tolerance = 1e-6;
    let result = variance::two_sample(&frame_two(case), "a", "b", Alternative::TwoSided, 0.95)
        .expect("two variances");
    let text = result.to_session_text();
    close(
        number(&text, "F"),
        case["f"].as_f64().expect("F"),
        tolerance,
        "F",
    );
    close(
        number(&text, "p-value"),
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    check_bounds(&text, &case["ci"], tolerance);
}
