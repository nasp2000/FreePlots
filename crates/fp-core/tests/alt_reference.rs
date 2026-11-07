//! Independent values for accelerated life testing.
//!
//! The values in `tests/reference/alt.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::alt::{self, AccelerationModel, LifeModel};
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/alt.json");
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

fn frame(case: &Value) -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("time", numbers(&case["times"])),
        Column::new_f64("stress", numbers(&case["stresses"])),
        Column::new_f64("censor", numbers(&case["events"])),
    ])
    .expect("frame")
}

fn check(case: &Value, life: LifeModel, acceleration: AccelerationModel) {
    let tolerance = reference()["tolerance"].as_f64().expect("tolerance");
    let df = frame(case);
    let model = alt::fit_with(
        &df,
        "time",
        "stress",
        Some("censor"),
        0.0,
        case["use_stress"].as_f64().expect("use stress"),
        life,
        acceleration,
    )
    .expect("accelerated life fit");
    assert_eq!(model.life, life);
    assert_eq!(model.acceleration, acceleration);
    assert!(model.converged);
    if let Some(expected) = case["shape"].as_f64() {
        close(model.shape.expect("shape"), expected, 1e-4, "shape");
    } else {
        assert!(model.shape.is_none());
    }
    close(
        model.intercept,
        case["intercept"].as_f64().expect("intercept"),
        1e-5,
        "intercept",
    );
    close(
        model.slope,
        case["slope"].as_f64().expect("slope"),
        1e-4,
        "slope",
    );
    close(
        model.centre,
        case["centre"].as_f64().expect("centre"),
        1e-12,
        "centre",
    );
    close(
        model.log_likelihood,
        case["log_likelihood"].as_f64().expect("log-likelihood"),
        tolerance,
        "log-likelihood",
    );
    close(
        model.life_at_use,
        case["life_at_use"].as_f64().expect("life at use"),
        1e-4,
        "life at use",
    );
    close(
        model.b10_at_use,
        case["b10_at_use"].as_f64().expect("B10"),
        1e-4,
        "B10",
    );
    close(
        model.acceleration_factor,
        case["acceleration_factor"]
            .as_f64()
            .expect("acceleration factor"),
        1e-4,
        "acceleration factor",
    );
    let dimension = if life == LifeModel::Exponential {
        2.0
    } else {
        3.0
    };
    close(
        model.aic,
        -2.0 * model.log_likelihood + 2.0 * dimension,
        tolerance,
        "AIC",
    );
    let events = case["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|value| value.as_f64().expect("event") != 0.0)
        .count();
    assert_eq!(model.events, events);
    assert_eq!(
        model.censored,
        case["times"].as_array().expect("times").len() - events
    );
}

#[test]
fn the_weibull_arrhenius_model_matches_the_likelihood() {
    let r = reference();
    check(
        &r["weibull"],
        LifeModel::Weibull,
        AccelerationModel::Arrhenius,
    );
}

#[test]
fn the_lognormal_inverse_power_model_matches_the_likelihood() {
    let r = reference();
    check(
        &r["lognormal"],
        LifeModel::Lognormal,
        AccelerationModel::InversePower,
    );
}

#[test]
fn the_exponential_eyring_model_matches_the_likelihood() {
    let r = reference();
    check(
        &r["exponential"],
        LifeModel::Exponential,
        AccelerationModel::Eyring,
    );
}

#[test]
fn invalid_designs_are_rejected() {
    let r = reference();
    let case = &r["weibull"];
    let df = frame(case);
    let use_stress = case["use_stress"].as_f64().expect("use stress");
    // A single stress level cannot identify the acceleration law.
    let single = DataFrame::new(vec![
        Column::new_f64("time", vec![10.0, 12.0, 14.0, 16.0]),
        Column::new_f64("stress", vec![350.0; 4]),
        Column::new_f64("censor", vec![1.0; 4]),
    ])
    .expect("frame");
    assert!(
        alt::fit_with(
            &single,
            "time",
            "stress",
            Some("censor"),
            0.0,
            use_stress,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius
        )
        .is_err()
    );
    // A non-positive use stress is rejected.
    assert!(
        alt::fit_with(
            &df,
            "time",
            "stress",
            Some("censor"),
            0.0,
            0.0,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius
        )
        .is_err()
    );
    // A non-positive lifetime is rejected.
    let mut bad = frame(case);
    bad.set_cell(0, 0, "0").expect("set cell");
    assert!(
        alt::fit_with(
            &bad,
            "time",
            "stress",
            Some("censor"),
            0.0,
            use_stress,
            LifeModel::Weibull,
            AccelerationModel::Arrhenius
        )
        .is_err()
    );
}
