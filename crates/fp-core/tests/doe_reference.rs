// SPDX-License-Identifier: MIT
//! Independent reference values for the saturated 2^3 factorial.
//!
//! The values in `tests/reference/doe.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::doe;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/doe.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn frame(case: &Value) -> DataFrame {
    let rows = case["rows"].as_array().expect("rows");
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    let mut y = Vec::new();
    for row in rows {
        let values = row.as_array().expect("row");
        a.push(values[0].as_f64().expect("A"));
        b.push(values[1].as_f64().expect("B"));
        c.push(values[2].as_f64().expect("C"));
        y.push(values[3].as_f64().expect("y"));
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("C", c),
        Column::new_f64("y", y),
    ])
    .expect("frame")
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

#[test]
fn the_saturated_factorial_matches_the_oracle() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model = doe::fit(
        &frame(&r),
        "y",
        &["A".to_owned(), "B".to_owned(), "C".to_owned()],
    )
    .expect("factorial");
    let terms: Vec<String> = r["terms"]
        .as_array()
        .expect("terms")
        .iter()
        .map(|value| value.as_str().expect("term").to_owned())
        .collect();
    assert_eq!(model.names, terms);
    let coefficients = numbers(&r["coefficients"]);
    let effects = numbers(&r["effects"]);
    let t_values = numbers(&r["t"]);
    let p_values = numbers(&r["p"]);
    let se = r["effect_se"].as_f64().expect("SE");
    for (index, name) in terms.iter().enumerate().skip(1) {
        close(
            model.coefficients[index],
            coefficients[index],
            tolerance,
            name,
        );
        close(model.effects[index], effects[index - 1], tolerance, name);
        close(model.effect_se[index], se, tolerance, name);
        close(model.t_values[index], t_values[index - 1], tolerance, name);
        close(model.p_values[index], p_values[index - 1], tolerance, name);
    }
    close(
        model.pure_error_ss,
        r["pure_error_ss"].as_f64().expect("pure SS"),
        tolerance,
        "pure error SS",
    );
    assert_eq!(
        model.pure_error_df,
        r["pure_error_df"].as_f64().expect("pure df")
    );
    assert_eq!(model.center_points, 2);

    let out = doe::analyze(
        &frame(&r),
        "y",
        &["A".to_owned(), "B".to_owned(), "C".to_owned()],
    )
    .expect("analysis");
    let text = out.results[0].to_session_text();
    close(
        statistic(&text, "R²"),
        r["r_squared"].as_f64().expect("R²"),
        tolerance,
        "R²",
    );
    close(
        statistic(&text, "Residual standard error"),
        r["residual_standard_error"].as_f64().expect("S"),
        tolerance,
        "S",
    );
    close(
        statistic(&text, "F"),
        r["f"].as_f64().expect("F"),
        tolerance,
        "F",
    );
    close(
        statistic(&text, "F p-value"),
        r["f_p"].as_f64().expect("F p"),
        tolerance,
        "F p",
    );
}

#[test]
fn a_value_outside_minus_one_plus_one_is_rejected() {
    let mut df = frame(&reference());
    df.set_cell(0, 0, "0.5").expect("set cell");
    assert!(doe::fit(&df, "y", &["A".to_owned(), "B".to_owned(), "C".to_owned()]).is_err());
}

#[test]
fn a_design_without_replication_or_centers_is_rejected() {
    let r = reference();
    let rows = r["rows"].as_array().expect("rows");
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    let mut y = Vec::new();
    for row in rows {
        let values = row.as_array().expect("row");
        if values[0].as_f64() == Some(0.0) {
            continue;
        }
        a.push(values[0].as_f64().expect("A"));
        b.push(values[1].as_f64().expect("B"));
        c.push(values[2].as_f64().expect("C"));
        y.push(values[3].as_f64().expect("y"));
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("C", c),
        Column::new_f64("y", y),
    ])
    .expect("frame");
    // A saturated design without center points has no error estimate: the fit
    // is still possible, but the effect tests are undefined.
    let model = doe::fit(&df, "y", &["A".to_owned(), "B".to_owned(), "C".to_owned()])
        .expect("saturated fit");
    assert_eq!(model.center_points, 0);
    assert!(model.effect_se[1].is_nan());
}
