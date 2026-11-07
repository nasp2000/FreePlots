// SPDX-License-Identifier: MIT
//! Independent reference values for the Hotelling T² and P charts.
//!
//! The values in `tests/reference/control2.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::attribute::{self, AttributeChartKind};
use fp_core::data::{Column, DataFrame};
use fp_core::mvchart;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/control2.json");
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
fn the_hotelling_chart_matches_the_pooled_covariance() {
    let r = reference();
    let case = &r["t2"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let rows = case["values"].as_array().expect("values");
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut z = Vec::new();
    for row in rows {
        let values = row.as_array().expect("row");
        x.push(values[0].as_f64().expect("x"));
        y.push(values[1].as_f64().expect("y"));
        z.push(values[2].as_f64().expect("z"));
    }
    let df = DataFrame::new(vec![
        Column::new_f64("x", x),
        Column::new_f64("y", y),
        Column::new_f64("z", z),
    ])
    .expect("frame");
    let model = mvchart::fit(
        &df,
        &["x".to_owned(), "y".to_owned(), "z".to_owned()],
        case["subgroup"].as_u64().expect("subgroup") as usize,
        case["confidence"].as_f64().expect("confidence"),
    )
    .expect("t2 chart");
    assert_eq!(model.groups, 10);
    assert_eq!(model.subgroup, 4);
    for (actual, expected) in model.mean.iter().zip(numbers(&case["mean"])) {
        close(*actual, expected, tolerance, "mean");
    }
    let covariance = case["covariance"].as_array().expect("covariance");
    for (row, expected) in covariance.iter().enumerate() {
        for (column, expected) in numbers(expected).into_iter().enumerate() {
            close(
                model.covariance[row][column],
                expected,
                tolerance,
                "covariance",
            );
        }
    }
    for (actual, expected) in model.values.iter().zip(numbers(&case["statistics"])) {
        close(*actual, expected, tolerance, "T²");
    }
    close(
        model.ucl,
        case["ucl"].as_f64().expect("UCL"),
        tolerance,
        "UCL",
    );
    close(
        model.f_critical,
        case["f_critical"].as_f64().expect("F"),
        tolerance,
        "F critical",
    );
    close(
        model.f_df,
        case["f_df"].as_f64().expect("df"),
        tolerance,
        "df",
    );
    let out_of_control: Vec<usize> = case["out_of_control"]
        .as_array()
        .expect("out of control")
        .iter()
        .map(|value| value.as_u64().expect("index") as usize)
        .collect();
    assert_eq!(model.out_of_control, out_of_control);
}

#[test]
fn the_p_chart_matches_the_binomial_limits() {
    let r = reference();
    let case = &r["pchart"];
    let tolerance = 1e-6;
    let df =
        DataFrame::new(vec![Column::new_f64("defects", numbers(&case["defects"]))]).expect("frame");
    let out = attribute::analyze(
        &df,
        AttributeChartKind::P,
        "defects",
        None,
        Some(case["size"].as_u64().expect("size") as usize),
    )
    .expect("p chart");
    let text = out.results[0].to_session_text();
    let statistic = |label: &str| -> f64 {
        let prefix: Vec<&str> = label.split_whitespace().collect();
        for line in text.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() > prefix.len()
                && tokens[..prefix.len()] == prefix[..]
                && let Ok(value) = tokens[prefix.len()].parse()
            {
                return value;
            }
        }
        panic!("no {label} in:\n{text}")
    };
    close(
        statistic("Center line"),
        case["center"].as_f64().expect("center"),
        tolerance,
        "center",
    );
    // The chart table row: P center LCL – LCL UCL – UCL.
    let row = text
        .lines()
        .find(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            tokens.len() > 2 && tokens[0] == "P" && tokens[1].parse::<f64>().is_ok()
        })
        .expect("chart row");
    let tokens: Vec<&str> = row.split_whitespace().collect();
    close(
        tokens[5].parse().expect("UCL"),
        case["ucl"].as_f64().expect("UCL"),
        tolerance,
        "UCL",
    );
    close(
        tokens[2].parse().expect("LCL"),
        case["lcl"].as_f64().expect("LCL"),
        tolerance,
        "LCL",
    );
    let signals: usize = statistic("Points outside limits") as usize;
    assert_eq!(
        signals,
        case["signals"].as_array().expect("signals").len(),
        "{text}"
    );
}

#[test]
fn invalid_inputs_are_rejected() {
    let r = reference();
    let case = &r["pchart"];
    let df =
        DataFrame::new(vec![Column::new_f64("defects", numbers(&case["defects"]))]).expect("frame");
    // A P chart needs a size.
    assert!(attribute::analyze(&df, AttributeChartKind::P, "defects", None, None).is_err());
    // A negative count is not a count.
    let mut bad = df.clone();
    bad.set_cell(0, 0, "-1").expect("set cell");
    assert!(attribute::analyze(&bad, AttributeChartKind::P, "defects", None, Some(50)).is_err());
    // Two variables are needed for T².
    let single = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0])]).expect("frame");
    assert!(mvchart::fit(&single, &["x".to_owned()], 2, 0.95).is_err());
}
