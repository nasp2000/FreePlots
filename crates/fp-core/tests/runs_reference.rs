// SPDX-License-Identifier: MIT
//! Independent values for Nelson's runs tests (2-4) on a C chart.
//!
//! The values in `tests/reference/runs.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::attribute::{self, AttributeChartKind};
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/runs.json");
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

fn statistic(text: &str, label: &str) -> f64 {
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
}

fn signals(text: &str) -> Vec<(usize, usize)> {
    text.lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() >= 6
                && let (Ok(test), Ok(point)) =
                    (tokens[0].parse::<usize>(), tokens[1].parse::<usize>())
                && (1..=4).contains(&test)
                && tokens[2].parse::<f64>().is_ok()
            {
                return Some((test, point));
            }
            None
        })
        .collect()
}

fn check(case: &Value) {
    let df =
        DataFrame::new(vec![Column::new_f64("defects", numbers(&case["points"]))]).expect("frame");
    let out =
        attribute::analyze(&df, AttributeChartKind::C, "defects", None, None).expect("C chart");
    let text = out.results[0].to_session_text();
    let expected: Vec<(usize, usize)> = case["signals"]
        .as_array()
        .expect("signals")
        .iter()
        .map(|signal| {
            (
                signal["test"].as_u64().expect("test") as usize,
                signal["index"].as_u64().expect("index") as usize,
            )
        })
        .collect();
    assert_eq!(signals(&text), expected, "{text}");
    assert_eq!(
        statistic(&text, "Signals") as usize,
        expected.len(),
        "{text}"
    );
    assert_eq!(
        statistic(&text, "Points outside limits"),
        0.0,
        "these cases have no point outside the limits:\n{text}"
    );
}

#[test]
fn nine_points_on_one_side_are_flagged() {
    check(&reference()["side"]);
}

#[test]
fn six_points_trending_are_flagged_after_a_reversal() {
    check(&reference()["trending"]);
}

#[test]
fn fourteen_alternating_points_are_flagged_after_a_break() {
    check(&reference()["alternating"]);
}

#[test]
fn a_tie_breaks_the_alternation() {
    check(&reference()["tie"]);
}

#[test]
fn a_short_series_has_no_signals() {
    let df = DataFrame::new(vec![Column::new_f64(
        "defects",
        vec![1.0, 2.0, 3.0, 2.0, 1.0],
    )])
    .expect("frame");
    let out =
        attribute::analyze(&df, AttributeChartKind::C, "defects", None, None).expect("C chart");
    let text = out.results[0].to_session_text();
    assert_eq!(statistic(&text, "Signals"), 0.0, "{text}");
    assert!(text.contains("No special-cause signals."), "{text}");
}
