//! Independent reference values for the Laney p'/c' charts.
//!
//! The values in `tests/reference/laney.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::attribute::{self, AttributeChartKind};
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/laney.json");
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

fn statistic(text: &str, label: &str) -> f64 {
    let prefix: Vec<&str> = label.split_whitespace().collect();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() > prefix.len() && tokens[..prefix.len()] == prefix[..] {
            for token in &tokens[prefix.len()..] {
                if let Ok(value) = token.parse() {
                    return value;
                }
            }
        }
    }
    panic!("no {label} in:\n{text}");
}

/// Parses the per-subgroup table: value, standardised, p', LCL and UCL.
fn subgroup_rows(text: &str) -> Vec<Vec<f64>> {
    text.lines()
        .filter_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() == 6
                && let (Ok(index), Ok(value), Ok(z), Ok(p), Ok(lcl), Ok(ucl)) = (
                    tokens[0].parse::<usize>(),
                    tokens[1].parse::<f64>(),
                    tokens[2].parse::<f64>(),
                    tokens[3].parse::<f64>(),
                    tokens[4].parse::<f64>(),
                    tokens[5].parse::<f64>(),
                )
            {
                let _ = index;
                return Some(vec![value, z, p, lcl, ucl]);
            }
            None
        })
        .collect()
}

fn check(kind: AttributeChartKind, counts: &[f64], case: &Value, rows: usize) {
    let tolerance = 1e-6;
    let df = DataFrame::new(vec![Column::new_f64("defects", counts.to_vec())]).expect("frame");
    let out = attribute::analyze(
        &df,
        kind,
        "defects",
        None,
        Some(reference()["size"].as_u64().expect("size") as usize),
    )
    .expect("laney chart");
    let text = out.results[0].to_session_text();
    close(
        statistic(&text, "Center line"),
        case["center"].as_f64().expect("center"),
        tolerance,
        "center",
    );
    close(
        statistic(&text, "Overdispersion factor"),
        case["sigma_z"].as_f64().expect("sigma_z"),
        tolerance,
        "sigma_z",
    );
    let table = subgroup_rows(&text);
    assert_eq!(table.len(), rows, "{text}");
    let z_expected = numbers(&case["z"]);
    let ucl = numbers(&case["ucl"]);
    let lcl = numbers(&case["lcl"]);
    for (index, row) in table.iter().enumerate() {
        close(row[0], numbers(&case["values"])[index], tolerance, "value");
        close(row[1], z_expected[index], 1e-4, "z");
        close(row[3], lcl[index], tolerance, "LCL");
        close(row[4], ucl[index], tolerance, "UCL");
    }
    let signals: usize = statistic(&text, "Points outside limits") as usize;
    assert_eq!(
        signals,
        case["signals"].as_array().expect("signals").len(),
        "{text}"
    );
}

#[test]
fn the_laney_p_chart_matches_the_moving_range_estimator() {
    let r = reference();
    let counts = numbers(&r["overdispersed"]);
    check(AttributeChartKind::LaneyP, &counts, &r["p"], counts.len());
}

#[test]
fn the_laney_c_chart_matches_the_moving_range_estimator() {
    let r = reference();
    let counts = numbers(&r["overdispersed"]);
    check(AttributeChartKind::LaneyC, &counts, &r["c"], counts.len());
}

#[test]
fn a_calm_process_keeps_the_ordinary_limits() {
    let r = reference();
    let counts = numbers(&r["calm"]);
    check(
        AttributeChartKind::LaneyP,
        &counts,
        &r["calm_p"],
        counts.len(),
    );
}

#[test]
fn invalid_inputs_are_rejected() {
    let r = reference();
    let counts = numbers(&r["overdispersed"]);
    // A Laney NP chart needs a constant subgroup size.
    let sizes: Vec<f64> = (0..counts.len()).map(|index| 90.0 + index as f64).collect();
    let df = DataFrame::new(vec![
        Column::new_f64("defects", counts),
        Column::new_f64("sizes", sizes),
    ])
    .expect("frame");
    assert!(
        attribute::analyze(
            &df,
            AttributeChartKind::LaneyNp,
            "defects",
            Some("sizes"),
            None
        )
        .is_err()
    );
    // Defectives cannot exceed the subgroup size.
    let too_big =
        DataFrame::new(vec![Column::new_f64("defects", vec![60.0, 70.0, 80.0])]).expect("frame");
    assert!(
        attribute::analyze(
            &too_big,
            AttributeChartKind::LaneyP,
            "defects",
            None,
            Some(50)
        )
        .is_err()
    );
}
