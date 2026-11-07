//! Independent reference values for the histogram, ECDF and Pareto chart.
//!
//! The values in `tests/reference/charts.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::{ecdf, histogram, pareto};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/charts.json");
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

fn frame(values: &[f64]) -> DataFrame {
    DataFrame::new(vec![Column::new_f64("value", values.to_vec())]).expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs oracle {expected}"
    );
}

fn check_histogram(case: &Value, values: &[f64], bins: Option<usize>) {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model = histogram::fit(&frame(values), "value", bins).expect("histogram");
    let edges = numbers(&case["edges"]);
    assert_eq!(model.edges.len(), edges.len());
    for (actual, expected) in model.edges.iter().zip(edges) {
        close(*actual, expected, tolerance, "edge");
    }
    let counts: Vec<usize> = case["counts"]
        .as_array()
        .expect("counts")
        .iter()
        .map(|value| value.as_u64().expect("count") as usize)
        .collect();
    assert_eq!(model.counts, counts);
    close(
        model.width,
        case["width"].as_f64().expect("width"),
        tolerance,
        "width",
    );
    assert_eq!(model.counts.iter().sum::<usize>(), model.n);
}

#[test]
fn histograms_match_the_bin_rules() {
    let r = reference();
    let values = numbers(&r["values"]);
    check_histogram(&r["histogram_auto"], &values, None);
    check_histogram(&r["histogram_7"], &values, Some(7));
}

#[test]
fn the_ecdf_matches_the_definitions() {
    let r = reference();
    let case = &r["ecdf"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let values = numbers(&r["values"]);
    let model =
        ecdf::from_values(&values, case["confidence"].as_f64().expect("confidence")).expect("ecdf");
    let probabilities = numbers(&case["probabilities"]);
    assert_eq!(model.probabilities.len(), probabilities.len());
    for (actual, expected) in model.probabilities.iter().zip(probabilities) {
        close(*actual, expected, tolerance, "probability");
    }
    close(
        model.epsilon,
        case["epsilon"].as_f64().expect("epsilon"),
        tolerance,
        "epsilon",
    );
    for (actual, expected) in model.quartiles.iter().zip(numbers(&case["quartiles"])) {
        close(*actual, expected, tolerance, "quartile");
    }
    close(
        model.largest_jump,
        case["largest_jump"].as_f64().expect("jump"),
        tolerance,
        "largest jump",
    );
}

#[test]
fn the_pareto_counts_match_the_oracle() {
    let r = reference();
    let case = &r["pareto"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let mut labels = Vec::new();
    for (category, count) in case["counts"].as_object().expect("counts") {
        for _ in 0..count.as_u64().expect("count") {
            labels.push(category.clone());
        }
    }
    let df = DataFrame::new(vec![Column::new_text("category", labels)]).expect("frame");
    let model = pareto::fit(&df, "category").expect("pareto");
    let categories: Vec<String> = case["categories"]
        .as_array()
        .expect("categories")
        .iter()
        .map(|value| value.as_str().expect("category").to_owned())
        .collect();
    assert_eq!(model.categories, categories);
    for (actual, expected) in model.counts.iter().zip(numbers(&case["values"])) {
        close(*actual, expected, tolerance, "count");
    }
    for (actual, expected) in model.percents.iter().zip(numbers(&case["percents"])) {
        close(*actual, expected, tolerance, "percent");
    }
    for (actual, expected) in model.cumulative.iter().zip(numbers(&case["cumulative"])) {
        close(*actual, expected, tolerance, "cumulative");
    }
    assert_eq!(
        model.vital_few,
        case["vital_few"].as_u64().expect("vital few") as usize
    );
    close(
        model.total,
        case["total"].as_f64().expect("total"),
        tolerance,
        "total",
    );
}

#[test]
fn invalid_inputs_are_rejected() {
    let constant = frame(&[5.0; 10]);
    assert!(histogram::fit(&constant, "value", None).is_err());
    let r = reference();
    let values = numbers(&r["values"]);
    assert!(histogram::fit(&frame(&values), "value", Some(0)).is_err());
    assert!(ecdf::from_values(&[], 0.95).is_err());
    assert!(ecdf::from_values(&values, 1.5).is_err());
}
