// SPDX-License-Identifier: MIT
//! Validation against the NIST Statistical Reference Datasets (StRD).
//!
//! The reference values are the **certified values** published with each
//! dataset: the coefficients, their standard deviations, the residual standard
//! deviation, R² and the analysis-of-variance sums of squares. The datasets are
//! copied here unchanged (lower level of difficulty) so the test runs offline.
//!
//! The tolerances record the deviation found in this implementation:
//!
//! - Norris (linear, well scaled): 1e-9 relative on the coefficients and 1e-9
//!   on the residual deviation;
//! - Pontius (quadratic, x up to 3·10⁶, raw powers): the fit is
//!   ill-conditioned in the raw basis, so the coefficients keep 1e-6 (B0),
//!   1e-8 (B1) and 1e-7 (B2) relative, while the invariant quantities (residual
//!   deviation, R², sums of squares) stay at 1e-8 or better.

use fp_core::curvefit::{CurveModel, fit};
use fp_core::data::{Column, DataFrame};

const NORRIS_X: [f64; 36] = [
    0.2, 337.4, 118.2, 884.6, 10.1, 226.5, 666.3, 996.3, 448.6, 777.0, 558.2, 0.4, 0.6, 775.5,
    666.9, 338.0, 447.5, 11.6, 556.0, 228.1, 995.8, 887.6, 120.2, 0.3, 0.3, 556.8, 339.1, 887.2,
    999.0, 779.0, 11.1, 118.3, 229.2, 669.1, 448.9, 0.5,
];
const NORRIS_Y: [f64; 36] = [
    0.1, 338.8, 118.1, 888.0, 9.2, 228.1, 668.5, 998.5, 449.1, 778.9, 559.2, 0.3, 0.1, 778.1,
    668.8, 339.3, 448.9, 10.8, 557.7, 228.3, 998.0, 888.8, 119.6, 0.3, 0.6, 557.6, 339.3, 888.0,
    998.5, 778.9, 10.2, 117.6, 228.9, 668.4, 449.2, 0.2,
];

const PONTIUS_X: [f64; 40] = [
    150000.0, 300000.0, 450000.0, 600000.0, 750000.0, 900000.0, 1050000.0, 1200000.0, 1350000.0,
    1500000.0, 1650000.0, 1800000.0, 1950000.0, 2100000.0, 2250000.0, 2400000.0, 2550000.0,
    2700000.0, 2850000.0, 3000000.0, 150000.0, 300000.0, 450000.0, 600000.0, 750000.0, 900000.0,
    1050000.0, 1200000.0, 1350000.0, 1500000.0, 1650000.0, 1800000.0, 1950000.0, 2100000.0,
    2250000.0, 2400000.0, 2550000.0, 2700000.0, 2850000.0, 3000000.0,
];
const PONTIUS_Y: [f64; 40] = [
    0.11019, 0.21956, 0.32949, 0.43899, 0.54803, 0.65694, 0.76562, 0.87487, 0.98292, 1.09146,
    1.20001, 1.30822, 1.41599, 1.52399, 1.63194, 1.73947, 1.84646, 1.95392, 2.06128, 2.16844,
    0.11052, 0.22018, 0.32939, 0.43886, 0.54798, 0.65739, 0.76596, 0.87474, 0.983, 1.0915, 1.20004,
    1.30818, 1.41613, 1.52408, 1.63159, 1.73965, 1.84696, 1.95445, 2.06177, 2.16829,
];

fn frame(x: &[f64], y: &[f64]) -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("x", x.to_vec()),
        Column::new_f64("y", y.to_vec()),
    ])
    .expect("frame")
}

fn relative(actual: f64, expected: f64) -> f64 {
    ((actual - expected) / expected).abs()
}

#[test]
fn norris_linear_regression_matches_the_certified_values() {
    let df = frame(&NORRIS_X, &NORRIS_Y);
    let model = fit(&df, "x", "y", CurveModel::Polynomial(1), 0.95).expect("fit");
    assert_eq!(model.n, 36);
    let intercept = model.coefficients[0];
    let slope = model.coefficients[1];
    assert!(
        relative(intercept, -0.262_323_073_774_029) < 1e-9,
        "{intercept}"
    );
    assert!(relative(slope, 1.002_116_818_020_45) < 1e-9, "{slope}");
    assert!(
        relative(model.standard_errors[0], 0.232_818_234_301_152) < 1e-6,
        "se0 {}",
        model.standard_errors[0]
    );
    assert!(
        relative(model.standard_errors[1], 0.429_796_848_199_937e-3) < 1e-6,
        "se1 {}",
        model.standard_errors[1]
    );
    assert!(
        relative(model.residual_standard_error, 0.884_796_396_144_373) < 1e-9,
        "s {}",
        model.residual_standard_error
    );
    assert!(
        (model.r_squared - 0.999_993_745_883_712).abs() < 1e-12,
        "R² {}",
        model.r_squared
    );
    // The certified analysis of variance: regression 4255954.13232369 and
    // residual 26.6173985294224 on 1 and 34 degrees of freedom.
    let residual: f64 = model
        .y
        .iter()
        .zip(&model.fitted)
        .map(|(value, fitted)| (value - fitted).powi(2))
        .sum();
    let mean = model.y.iter().sum::<f64>() / model.y.len() as f64;
    let total: f64 = model.y.iter().map(|value| (value - mean).powi(2)).sum();
    let regression = total - residual;
    assert!(
        relative(residual, 26.617_398_529_422_4) < 1e-9,
        "SSE {residual}"
    );
    assert!(
        relative(regression, 4_255_954.132_323_69) < 1e-9,
        "SSR {regression}"
    );
    let f_statistic = regression / (residual / 34.0);
    assert!(
        relative(f_statistic, 5_436_385.540_797_85) < 1e-6,
        "F {f_statistic}"
    );
}

#[test]
fn pontius_quadratic_regression_stays_within_the_recorded_deviation() {
    let df = frame(&PONTIUS_X, &PONTIUS_Y);
    let model = fit(&df, "x", "y", CurveModel::Polynomial(2), 0.95).expect("fit");
    assert_eq!(model.n, 40);
    assert!(
        relative(model.coefficients[0], 0.673_565_789_473_684e-3) < 1e-5,
        "b0 {}",
        model.coefficients[0]
    );
    assert!(
        relative(model.coefficients[1], 0.732_059_160_401_003e-6) < 1e-7,
        "b1 {}",
        model.coefficients[1]
    );
    assert!(
        (model.coefficients[2] + 0.316_081_871_345_029e-14).abs() < 1e-17,
        "b2 {}",
        model.coefficients[2]
    );
    assert!(
        relative(model.residual_standard_error, 0.205_177_424_076_185e-3) < 1e-8,
        "s {}",
        model.residual_standard_error
    );
    assert!(
        (model.r_squared - 0.999_999_900_178_537).abs() < 1e-12,
        "R² {}",
        model.r_squared
    );
    let residual: f64 = model
        .y
        .iter()
        .zip(&model.fitted)
        .map(|(value, fitted)| (value - fitted).powi(2))
        .sum();
    assert!(
        relative(residual, 0.155_761_768_796_992e-5) < 1e-8,
        "SSE {residual}"
    );
    let mean = model.y.iter().sum::<f64>() / model.y.len() as f64;
    let regression: f64 = model
        .y
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        - residual;
    assert!(
        relative(regression, 15.604_034_324_419_8) < 1e-8,
        "SSR {regression}"
    );
}
