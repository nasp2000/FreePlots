// SPDX-License-Identifier: MIT
//! Independent reference values for the Box–Cox normal identification.
//!
//! The values in `tests/reference/boxcox_id.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::boxcox;
use fp_core::data::Sample;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/boxcox_id.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs reference {expected}"
    );
}

fn sample(values: &Value) -> Sample {
    Sample::new(
        values
            .as_array()
            .expect("values")
            .iter()
            .map(|value| value.as_f64().expect("value"))
            .collect(),
        fp_core::data::MissingPolicy::Drop,
    )
    .expect("sample")
}

#[test]
fn the_box_cox_normal_identification_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let identification = boxcox::identify_normal(&sample(&r["values"])).expect("identification");
    close(
        identification.lambda,
        r["lambda"].as_f64().expect("lambda"),
        tolerance,
        "lambda",
    );
    close(
        identification.mu,
        r["mu"].as_f64().expect("mu"),
        tolerance,
        "mu",
    );
    close(
        identification.sigma,
        r["sigma"].as_f64().expect("sigma"),
        tolerance,
        "sigma",
    );
    close(
        identification.a2,
        r["a2"].as_f64().expect("a2"),
        tolerance,
        "AD",
    );
    close(
        identification.log_likelihood,
        r["log_likelihood"].as_f64().expect("log likelihood"),
        tolerance,
        "log likelihood",
    );
    assert_eq!(
        identification.n_params,
        r["n_params"].as_u64().expect("n params") as usize
    );
}
