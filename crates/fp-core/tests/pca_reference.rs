//! Independent reference values for the principal components .
//!
//! The values in `tests/reference/pca.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::pca;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/pca.json");
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
    let rows = case["values"].as_array().expect("values");
    let columns = case["columns"].as_array().expect("columns");
    let mut data: Vec<Vec<f64>> = vec![Vec::with_capacity(rows.len()); columns.len()];
    for row in rows {
        let values = numbers(row);
        for (column, value) in values.into_iter().enumerate() {
            data[column].push(value);
        }
    }
    let columns = columns
        .iter()
        .zip(data)
        .map(|(name, values)| Column::new_f64(name.as_str().expect("name").to_owned(), values))
        .collect();
    DataFrame::new(columns).expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs reference {expected}"
    );
}

#[test]
fn principal_components_match_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = frame(&r);
    let names: Vec<String> = r["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|value| value.as_str().expect("name").to_owned())
        .collect();
    let model = pca::fit(&df, &names, names.len()).expect("pca");

    for (index, expected) in numbers(&r["means"]).into_iter().enumerate() {
        close(model.means[index], expected, tolerance, "mean");
    }
    for (index, expected) in numbers(&r["sds"]).into_iter().enumerate() {
        close(model.sds[index], expected, tolerance, "sd");
    }
    for (index, expected) in numbers(&r["eigenvalues"]).into_iter().enumerate() {
        close(model.eigenvalues[index], expected, tolerance, "eigenvalue");
    }
    for (index, expected) in numbers(&r["explained"]).into_iter().enumerate() {
        close(model.explained[index], expected, tolerance, "explained");
    }
    for (index, expected) in numbers(&r["cumulative"]).into_iter().enumerate() {
        close(model.cumulative[index], expected, tolerance, "cumulative");
    }

    let loadings = r["loadings"].as_array().expect("loadings");
    for (component, expected) in loadings.iter().enumerate() {
        for (variable, expected) in numbers(expected).into_iter().enumerate() {
            close(
                model.loadings[component][variable],
                expected,
                tolerance,
                "loading",
            );
        }
    }

    let scores = r["scores"].as_array().expect("scores");
    for (component, expected) in scores.iter().enumerate() {
        for (row, expected) in numbers(expected).into_iter().enumerate() {
            close(model.scores[component][row], expected, tolerance, "score");
        }
    }
}

#[test]
fn fewer_components_keep_the_leading_ones() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = frame(&r);
    let names: Vec<String> = r["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .map(|value| value.as_str().expect("name").to_owned())
        .collect();
    let model = pca::fit(&df, &names, 2).expect("pca");
    assert_eq!(model.scores.len(), 2);
    for (index, expected) in numbers(&r["eigenvalues"]).into_iter().take(2).enumerate() {
        close(model.eigenvalues[index], expected, tolerance, "eigenvalue");
    }
}

#[test]
fn a_single_variable_is_rejected() {
    let r = reference();
    let df = frame(&r);
    assert!(pca::fit(&df, &["v1".to_owned()], 1).is_err());
}
