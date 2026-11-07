//! Independent reference values for the logistic and Poisson fits.
//!
//! The values in `tests/reference/regression.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::logistic;
use fp_core::poisson;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/regression.json");
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
    let categories: Vec<String> = case["cat"]
        .as_array()
        .expect("categories")
        .iter()
        .map(|value| value.as_str().expect("category").to_owned())
        .collect();
    DataFrame::new(vec![
        Column::new_f64("x1", numbers(&case["x1"])),
        Column::new_f64("x2", numbers(&case["x2"])),
        Column::new_text("cat", categories),
        Column::new_f64("logistic_response", numbers(&case["logistic_response"])),
        Column::new_f64("poisson_response", numbers(&case["poisson_response"])),
    ])
    .expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs reference {expected}"
    );
}

struct TermStat<'a> {
    name: &'a str,
    coefficient: f64,
    standard_error: f64,
    z: f64,
    p: f64,
}

fn check_fit(stats: &[TermStat], expected: &Value, tolerance: f64, label: &str) {
    let expected_terms: Vec<String> = expected["terms"]
        .as_array()
        .expect("terms")
        .iter()
        .map(|value| value.as_str().expect("term").to_owned())
        .collect();
    let names: Vec<&str> = stats.iter().map(|stat| stat.name).collect();
    assert_eq!(names, expected_terms, "{label} terms");
    for (index, name) in expected_terms.iter().enumerate() {
        close(
            stats[index].coefficient,
            expected["coefficients"].as_array().expect("coefficients")[index]
                .as_f64()
                .expect("coefficient"),
            tolerance,
            &format!("{label} coefficient {name}"),
        );
        close(
            stats[index].standard_error,
            expected["standard_errors"].as_array().expect("se")[index]
                .as_f64()
                .expect("se"),
            tolerance,
            &format!("{label} SE {name}"),
        );
        close(
            stats[index].z,
            expected["z"].as_array().expect("z")[index]
                .as_f64()
                .expect("z"),
            tolerance,
            &format!("{label} z {name}"),
        );
        close(
            stats[index].p,
            expected["p_values"].as_array().expect("p")[index]
                .as_f64()
                .expect("p"),
            tolerance,
            &format!("{label} p {name}"),
        );
    }
}

#[test]
fn logistic_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = frame(&r["data"]);
    let model = logistic::fit(
        &df,
        "logistic_response",
        &["x1".to_owned(), "x2".to_owned()],
    )
    .expect("logistic fit");
    let expected = &r["logistic"];
    let stats: Vec<TermStat> = model
        .terms
        .iter()
        .enumerate()
        .map(|(index, name)| TermStat {
            name,
            coefficient: model.coefficients[index],
            standard_error: model.standard_errors[index],
            z: model.z[index],
            p: model.p_values[index],
        })
        .collect();
    check_fit(&stats, expected, tolerance, "logistic");
    close(
        model.log_likelihood,
        expected["log_likelihood"].as_f64().expect("llf"),
        tolerance,
        "logistic llf",
    );
    close(
        model.aic,
        expected["aic"].as_f64().expect("aic"),
        tolerance,
        "logistic AIC",
    );
    close(
        model.mcfadden_r2,
        expected["mcfadden_r2"].as_f64().expect("R2"),
        tolerance,
        "logistic McFadden",
    );
    let predicted = numbers(&expected["predicted"]);
    for (actual, expected) in model.predicted.iter().zip(predicted) {
        close(*actual, expected, tolerance, "logistic predicted");
    }
}

#[test]
fn logistic_dummy_coding_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = frame(&r["data"]);
    let model = logistic::fit(
        &df,
        "logistic_response",
        &["x1".to_owned(), "cat".to_owned()],
    )
    .expect("logistic fit");
    let expected = &r["logistic_categorical"];
    let stats: Vec<TermStat> = model
        .terms
        .iter()
        .enumerate()
        .map(|(index, name)| TermStat {
            name,
            coefficient: model.coefficients[index],
            standard_error: model.standard_errors[index],
            z: model.z[index],
            p: model.p_values[index],
        })
        .collect();
    check_fit(&stats, expected, tolerance, "logistic categorical");
    close(
        model.log_likelihood,
        expected["log_likelihood"].as_f64().expect("llf"),
        tolerance,
        "categorical llf",
    );
}

#[test]
fn poisson_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let df = frame(&r["data"]);
    let model = poisson::fit(&df, "poisson_response", &["x1".to_owned(), "x2".to_owned()])
        .expect("poisson fit");
    let expected = &r["poisson"];
    let stats: Vec<TermStat> = model
        .terms
        .iter()
        .enumerate()
        .map(|(index, name)| TermStat {
            name,
            coefficient: model.coefficients[index],
            standard_error: model.standard_errors[index],
            z: model.z[index],
            p: model.p_values[index],
        })
        .collect();
    check_fit(&stats, expected, tolerance, "poisson");
    close(
        model.log_likelihood,
        expected["log_likelihood"].as_f64().expect("llf"),
        tolerance,
        "poisson llf",
    );
    close(
        model.deviance,
        expected["deviance"].as_f64().expect("deviance"),
        tolerance,
        "poisson deviance",
    );
    close(
        model.pearson,
        expected["pearson"].as_f64().expect("pearson"),
        tolerance,
        "poisson Pearson",
    );
    close(
        model.dispersion,
        expected["dispersion"].as_f64().expect("dispersion"),
        tolerance,
        "poisson dispersion",
    );
    let fitted = numbers(&expected["fitted"]);
    for (actual, expected) in model.fitted.iter().zip(fitted) {
        close(*actual, expected, tolerance, "poisson fitted");
    }
}
