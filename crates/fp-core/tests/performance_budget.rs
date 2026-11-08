// SPDX-License-Identifier: MIT
//! Wall-clock budgets for analyses that must scale to a million rows.
//!
//! The test is compiled only in release builds: a debug build is 10–50×
//! slower and would not measure the shipped binary. The `performance` job in
//! CI runs `cargo test --release`; the normal debug suite skips it.

#![cfg(not(debug_assertions))]

use std::time::{Duration, Instant};

use fp_core::data::{Column, DataFrame};
use fp_core::spc::ControlChartKind;
use fp_core::workflow::{AnalysisSpec, run_analysis};

/// Per-analysis budget. `FREEPLOTS_ANALYSIS_BUDGET_MS` overrides it on a
/// slower CI runner; the Raspberry Pi 4 gets a wider default.
fn budget() -> Duration {
    let default_ms = if cfg!(target_arch = "aarch64") {
        8000
    } else {
        3000
    };
    let ms = std::env::var("FREEPLOTS_ANALYSIS_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_ms);
    Duration::from_millis(ms)
}

fn million_rows() -> DataFrame {
    let rows = 1_000_000usize;
    let x: Vec<f64> = (0..rows).map(|row| row as f64 * 0.001).collect();
    let y: Vec<f64> = x
        .iter()
        .enumerate()
        .map(|(row, value)| 2.0 * value + ((row % 17) as f64) * 0.01)
        .collect();
    DataFrame::new(vec![
        Column::new_f64("X", x.clone()),
        Column::new_f64("Y", y),
        Column::new_f64("C1", x),
    ])
    .expect("frame")
}

#[test]
fn one_million_rows_stay_inside_the_analysis_budgets() {
    let df = million_rows();
    let limit = budget();
    let cases: Vec<(&str, AnalysisSpec)> = vec![
        ("descriptive", AnalysisSpec::descriptive_statistics("C1")),
        (
            "capability",
            AnalysisSpec::capability("C1", Some(-10.0), Some(1010.0)),
        ),
        (
            "I-MR",
            AnalysisSpec::control_chart("C1", ControlChartKind::IndividualsMovingRange, 1),
        ),
        (
            "regression",
            AnalysisSpec::regression("Y", vec!["X".to_owned()]),
        ),
    ];
    for (name, spec) in cases {
        let started = Instant::now();
        let output = run_analysis(&df, &spec).unwrap_or_else(|error| panic!("{name}: {error}"));
        let elapsed = started.elapsed();
        println!("{name}: {elapsed:?} (budget {limit:?})");
        assert!(!output.results.is_empty(), "{name}: empty result");
        assert!(
            elapsed <= limit,
            "{name} took {elapsed:?}, budget {limit:?}"
        );
    }
}
