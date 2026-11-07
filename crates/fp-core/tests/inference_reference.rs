// SPDX-License-Identifier: MIT
//! Independent reference values for the independence and variance tests.
//!
//! The values in `tests/reference/inference.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::contingency;
use fp_core::data::{Column, DataFrame};
use fp_core::variance::{self, VarianceCenter};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/inference.json");
    serde_json::from_str(raw).expect("valid JSON reference")
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

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs reference {expected}"
    );
}

fn contingency_frame(table: &Value) -> DataFrame {
    let rows = table.as_array().expect("table");
    let mut row_labels = Vec::new();
    let mut column_labels = Vec::new();
    for (row_index, counts) in rows.iter().enumerate() {
        for (column_index, count) in counts.as_array().expect("row").iter().enumerate() {
            for _ in 0..count.as_u64().expect("count") {
                row_labels.push(format!("R{row_index}"));
                column_labels.push(format!("C{column_index}"));
            }
        }
    }
    DataFrame::new(vec![
        Column::new_text("row", row_labels),
        Column::new_text("column", column_labels),
    ])
    .expect("frame")
}

#[test]
fn chi_square_matches_reference() {
    let r = reference();
    let case = &r["chi_square"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model =
        contingency::test(&contingency_frame(&case["table"]), "row", "column").expect("chi-square");
    close(
        model.statistic,
        case["statistic"].as_f64().expect("statistic"),
        tolerance,
        "chi-square",
    );
    close(
        model.p_value,
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    assert_eq!(
        model.degrees_of_freedom,
        case["degrees_of_freedom"].as_u64().expect("df") as usize
    );
    // Cramér's V is private in the struct; recompute it from the statistic.
    let n = model.n as f64;
    let v = (model.statistic / (n * 1.0)).sqrt();
    close(
        v,
        case["cramers_v"].as_f64().expect("V"),
        tolerance,
        "Cramér's V",
    );
}

#[test]
fn fisher_exact_matches_reference() {
    let r = reference();
    let case = &r["fisher"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let table = case["table"].as_array().expect("table");
    let mut counts = [[0.0f64; 2]; 2];
    for (row, values) in counts.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            *value = table[row].as_array().expect("row")[column]
                .as_f64()
                .expect("count");
        }
    }
    let model = contingency::fisher_exact(counts).expect("fisher");
    close(
        model.odds_ratio,
        case["odds_ratio"].as_f64().expect("odds ratio"),
        tolerance,
        "odds ratio",
    );
    close(
        model.two_sided,
        case["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
}

fn variance_frame(groups: &Value) -> DataFrame {
    let mut values = Vec::new();
    let mut labels = Vec::new();
    for (label, group) in groups.as_object().expect("groups") {
        for value in group.as_array().expect("group") {
            values.push(value.as_f64().expect("observation"));
            labels.push(label.clone());
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", values),
        Column::new_text("g", labels),
    ])
    .expect("frame")
}

#[test]
fn levene_matches_reference_for_both_centres() {
    let r = reference();
    // The Session report prints six decimals, so this comparison is at the
    // printed precision, not at the full floating-point precision.
    let tolerance = 1e-6;
    let frame = variance_frame(&r["groups"]);
    for (center, key) in [
        (VarianceCenter::Mean, "mean"),
        (VarianceCenter::Median, "median"),
    ] {
        let case = &r["levene"][key];
        let result = variance::levene(&frame, "y", "g", center, 0.95).expect("levene");
        let text = result.to_session_text();
        close(
            number(&text, "F"),
            case["statistic"].as_f64().expect("F"),
            tolerance,
            key,
        );
        close(
            number(&text, "p-value"),
            case["p_value"].as_f64().expect("p"),
            tolerance,
            key,
        );
    }
}

#[test]
fn bartlett_matches_reference() {
    let r = reference();
    // The Session report prints six decimals, so this comparison is at the
    // printed precision, not at the full floating-point precision.
    let tolerance = 1e-6;
    let frame = variance_frame(&r["groups"]);
    let result = variance::bartlett(&frame, "y", "g").expect("bartlett");
    let text = result.to_session_text();
    close(
        number(&text, "Chi-square"),
        r["bartlett"]["statistic"].as_f64().expect("statistic"),
        tolerance,
        "Bartlett",
    );
    close(
        number(&text, "p-value"),
        r["bartlett"]["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
}
