// SPDX-License-Identifier: MIT
//! Wall-clock budgets for analyses that must scale to a million rows.
//!
//! The test is compiled only in release builds: a debug build is 10–50×
//! slower and would not measure the shipped binary. The `performance` job in
//! CI runs `cargo test --release`; the normal debug suite skips it.

#![cfg(not(debug_assertions))]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use fp_core::data::{Column, DataFrame};
use fp_core::io;
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

/// Budget for reading one million rows from text.
fn import_budget() -> Duration {
    let default_ms = if cfg!(target_arch = "aarch64") {
        20_000
    } else {
        10_000
    };
    let ms = std::env::var("FREEPLOTS_IMPORT_BUDGET_MS")
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
    let z: Vec<f64> = (0..rows)
        .map(|row| ((row % 1_000) as f64 / 100.0).sin())
        .collect();
    let group: Vec<f64> = (0..rows).map(|row| (row % 3) as f64).collect();
    let binary: Vec<f64> = (0..rows).map(|row| (row % 2) as f64).collect();
    let w: Vec<f64> = (0..rows).map(|row| (row as f64 * 0.000_7).cos()).collect();
    DataFrame::new(vec![
        Column::new_f64("X", x.clone()),
        Column::new_f64("Y", y),
        Column::new_f64("Z", z),
        Column::new_f64("W", w),
        Column::new_f64("G", group),
        Column::new_f64("B", binary),
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
            "Xbar-R",
            AnalysisSpec::control_chart("C1", ControlChartKind::XbarR, 5),
        ),
        (
            "regression",
            AnalysisSpec::regression("Y", vec!["X".to_owned()]),
        ),
        (
            "regression 2",
            AnalysisSpec::regression("Y", vec!["X".to_owned(), "Z".to_owned()]),
        ),
        (
            "logistic",
            AnalysisSpec::logistic("B", vec!["X".to_owned(), "Z".to_owned()]),
        ),
        ("ANOVA", AnalysisSpec::anova("C1", "G")),
        (
            "PCA",
            AnalysisSpec::pca(vec!["X".to_owned(), "Z".to_owned(), "W".to_owned()], 2),
        ),
        ("moving average", AnalysisSpec::moving_average("C1", 1_001)),
        ("scatterplot", AnalysisSpec::scatterplot("X", "Y")),
        ("one-sample t", AnalysisSpec::one_sample_t("C1", 500.0)),
        ("two-sample t", AnalysisSpec::two_sample_t("X", "Y")),
        ("histogram", AnalysisSpec::histogram("C1")),
        ("boxplot", AnalysisSpec::boxplot("C1")),
        ("probability plot", AnalysisSpec::probability_plot("C1")),
        ("ECDF", AnalysisSpec::ecdf("C1")),
        (
            "correlation",
            AnalysisSpec::correlation(vec!["X".to_owned(), "Y".to_owned()]),
        ),
        ("normality", AnalysisSpec::normality_test("C1")),
        ("run chart", AnalysisSpec::run_chart("C1")),
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

#[test]
fn one_million_rows_import_stays_inside_the_budget() {
    let rows = 1_000_000usize;
    let mut text = String::with_capacity(rows * 20);
    text.push_str("X,Y\n");
    for row in 0..rows {
        let _ = writeln!(text, "{}.25,{}", row, row % 1_000);
    }

    let limit = import_budget();
    let started = Instant::now();
    let options = io::detect_options(&text);
    let df = io::read_csv_str(&text, options).expect("import");
    let elapsed = started.elapsed();
    println!("1M rows import: {elapsed:?} (budget {limit:?})");
    assert_eq!(df.n_rows(), rows);
    assert_eq!(df.n_columns(), 2);
    assert!(
        elapsed <= limit,
        "the million-row import took {elapsed:?}, budget {limit:?}"
    );
}
