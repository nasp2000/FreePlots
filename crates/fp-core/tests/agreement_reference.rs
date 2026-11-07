// SPDX-License-Identifier: MIT
//! Independent reference values for attribute agreement (Cohen's kappa).
//!
//! The values in `tests/reference/agreement.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::agreement;
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/agreement.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn frame(case: &Value) -> DataFrame {
    let rows = case["rows"].as_array().expect("rows");
    let mut part = Vec::new();
    let mut appraiser = Vec::new();
    let mut rating = Vec::new();
    for row in rows {
        part.push(row["part"].as_str().expect("part").to_owned());
        appraiser.push(row["appraiser"].as_str().expect("appraiser").to_owned());
        rating.push(row["rating"].as_str().expect("rating").to_owned());
    }
    DataFrame::new(vec![
        Column::new_text("part", part),
        Column::new_text("appraiser", appraiser),
        Column::new_text("rating", rating),
    ])
    .expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs oracle {expected}"
    );
}

#[test]
fn attribute_agreement_matches_the_hand_kappa() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model = agreement::fit(&frame(&r), "part", "appraiser", "rating").expect("agreement");
    assert_eq!(model.n, 54);
    assert_eq!(model.parts.len(), 6);
    assert_eq!(model.appraisers.len(), 3);

    for within in &model.within {
        let expected = &r["within"][&within.appraiser];
        assert_eq!(
            within.parts,
            expected["n"].as_u64().expect("n") as usize,
            "{}",
            within.appraiser
        );
        close(
            within.kappa.observed,
            expected["observed"].as_f64().expect("observed"),
            tolerance,
            "observed",
        );
        close(
            within.kappa.expected,
            expected["expected"].as_f64().expect("expected"),
            tolerance,
            "expected",
        );
        close(
            within.kappa.kappa,
            expected["kappa"].as_f64().expect("kappa"),
            tolerance,
            "kappa",
        );
    }

    for between in &model.between {
        let key = format!("{}-{}", between.first, between.second);
        let expected = &r["between"][&key];
        close(
            between.kappa.observed,
            expected["observed"].as_f64().expect("observed"),
            tolerance,
            "between observed",
        );
        close(
            between.kappa.expected,
            expected["expected"].as_f64().expect("expected"),
            tolerance,
            "between expected",
        );
        close(
            between.kappa.kappa,
            expected["kappa"].as_f64().expect("kappa"),
            tolerance,
            "between kappa",
        );
    }
}

#[test]
fn a_missing_rating_is_excluded() {
    let r = reference();
    let mut df = frame(&r);
    df.set_missing(2, 0).expect("set missing");
    let model = agreement::fit(&df, "part", "appraiser", "rating").expect("agreement");
    assert_eq!(model.n, 53);
}

#[test]
fn repeated_columns_and_single_appraisers_are_rejected() {
    let r = reference();
    let df = frame(&r);
    assert!(agreement::fit(&df, "part", "part", "rating").is_err());

    let rows = r["rows"].as_array().expect("rows");
    let mut part = Vec::new();
    let mut appraiser = Vec::new();
    let mut rating = Vec::new();
    for row in rows {
        if row["appraiser"].as_str() != Some("A") {
            continue;
        }
        part.push(row["part"].as_str().expect("part").to_owned());
        appraiser.push("A".to_owned());
        rating.push(row["rating"].as_str().expect("rating").to_owned());
    }
    let single = DataFrame::new(vec![
        Column::new_text("part", part),
        Column::new_text("appraiser", appraiser),
        Column::new_text("rating", rating),
    ])
    .expect("frame");
    assert!(agreement::fit(&single, "part", "appraiser", "rating").is_err());
}
