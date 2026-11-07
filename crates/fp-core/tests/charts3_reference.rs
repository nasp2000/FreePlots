// SPDX-License-Identifier: MIT
//! Independent values for the Mann–Kendall trend and the interval-plot wiring.

use fp_core::anova;
use fp_core::data::{Column, DataFrame};
use fp_core::intervalplot;
use fp_core::runchart;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/trend.json");
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
fn mann_kendall_matches_the_definitions() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let values = numbers(&r["values"]);
    let trend = runchart::mann_kendall(&values).expect("trend");
    close(
        trend.statistic,
        r["statistic"].as_f64().expect("S"),
        tolerance,
        "S",
    );
    close(trend.z, r["z"].as_f64().expect("z"), tolerance, "z");
    close(
        trend.p_value,
        r["p_value"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    close(
        trend.slope,
        r["slope"].as_f64().expect("slope"),
        tolerance,
        "Theil–Sen slope",
    );
}

#[test]
fn invalid_series_are_rejected() {
    assert!(runchart::mann_kendall(&[1.0, 2.0, 3.0]).is_err());
    assert!(runchart::mann_kendall(&[5.0; 10]).is_err());
}

#[test]
fn the_interval_plot_uses_the_anova_intervals() {
    let response = vec![
        10.0, 10.4, 9.8, 10.2, 10.1, 10.6, 9.9, 10.3, 11.4, 11.8, 11.2, 11.6, 11.5, 11.9, 11.3,
        11.7,
    ];
    let factor = vec![
        "A", "A", "A", "A", "A", "A", "A", "A", "B", "B", "B", "B", "B", "B", "B", "B",
    ];
    let df = DataFrame::new(vec![
        Column::new_f64("y", response),
        Column::new_text(
            "g",
            factor.iter().map(|value| (*value).to_owned()).collect(),
        ),
    ])
    .expect("frame");
    let model = anova::fit(&df, "y", "g", 0.95).expect("anova");
    let out = intervalplot::analyze(&df, "y", "g", 0.95).expect("interval plot");
    let plot = out.plot.expect("plot");
    assert_eq!(plot.points.len(), model.groups.len());
    for (index, group) in model.groups.iter().enumerate() {
        close(plot.points[index].y, group.mean, 1e-12, "mean point");
        let x = index as f64 + 1.0;
        let segment = plot
            .lines
            .iter()
            .find(|segment| {
                (segment.from.x - x).abs() < 1e-12
                    && (segment.to.x - x).abs() < 1e-12
                    && segment.from.y != segment.to.y
            })
            .unwrap_or_else(|| panic!("no interval segment for {x}"));
        let (low, high) = if segment.from.y < segment.to.y {
            (segment.from.y, segment.to.y)
        } else {
            (segment.to.y, segment.from.y)
        };
        close(low, group.ci_lower, 1e-12, "CI lower");
        close(high, group.ci_upper, 1e-12, "CI upper");
    }
}
