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
use fp_core::timeseries::DecompositionModel;
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

/// Budget for the analyses whose cost is dominated by iterative fits (a
/// maximum-likelihood search or a variable selection) rather than by a single
/// pass over the rows; they are measured at a hundred thousand rows.
fn iterative_budget() -> Duration {
    let default_ms = if cfg!(target_arch = "aarch64") {
        40_000
    } else {
        20_000
    };
    let ms = std::env::var("FREEPLOTS_ITERATIVE_BUDGET_MS")
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
    frame_with_rows(1_000_000)
}

fn frame_with_rows(rows: usize) -> DataFrame {
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
    let survival: Vec<f64> = (0..rows).map(|row| 1.0 + row as f64 * 0.001).collect();
    let stress: Vec<f64> = (0..rows)
        .map(|row| [300.0, 350.0, 400.0, 450.0][row % 4])
        .collect();
    // Counts for the Poisson model and a three-level ordinal response; both
    // cycle through the rows so the fit has a well-defined optimum.
    let counts: Vec<f64> = (0..rows).map(|row| (row % 7) as f64).collect();
    let ordinal: Vec<f64> = (0..rows).map(|row| (row % 3) as f64).collect();
    DataFrame::new(vec![
        Column::new_f64("X", x.clone()),
        Column::new_f64("Y", y),
        Column::new_f64("Z", z),
        Column::new_f64("W", w),
        Column::new_f64("G", group),
        Column::new_f64("B", binary),
        Column::new_f64("T", survival),
        Column::new_f64("S", stress),
        Column::new_f64("K", counts),
        Column::new_f64("O", ordinal),
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
            "Box-Cox capability",
            AnalysisSpec::capability("T", Some(0.5), Some(1001.0)).with_capability_box_cox(true),
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
        (
            "K-means",
            AnalysisSpec::kmeans(vec!["X".to_owned(), "Z".to_owned(), "W".to_owned()], 3),
        ),
        ("moving average", AnalysisSpec::moving_average("C1", 1_001)),
        (
            "smoothing",
            AnalysisSpec::exponential_smoothing("C1", 12, 12),
        ),
        (
            "decomposition",
            AnalysisSpec::decomposition("C1", 12, DecompositionModel::Additive),
        ),
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
        ("Kaplan-Meier", AnalysisSpec::kaplan_meier("T")),
        (
            "correlation matrix",
            AnalysisSpec::correlation(vec!["X".to_owned(), "Y".to_owned(), "Z".to_owned()]),
        ),
        (
            "partial correlation",
            AnalysisSpec::partial_correlation("X", "Y", "Z"),
        ),
        ("tolerance interval", AnalysisSpec::tolerance_interval("C1")),
        ("paired t", AnalysisSpec::paired_t("X", "Y")),
        (
            "one-sample equivalence",
            AnalysisSpec::one_sample_equivalence("C1", 500.0, 10.0),
        ),
        ("Mann-Whitney", AnalysisSpec::mann_whitney("X", "Y")),
        (
            "Wilcoxon signed-rank",
            AnalysisSpec::wilcoxon_signed_rank("X", "Y"),
        ),
        ("Kruskal-Wallis", AnalysisSpec::kruskal_wallis("C1", "G")),
        (
            "Friedman",
            AnalysisSpec::friedman(vec!["X".to_owned(), "Y".to_owned(), "Z".to_owned()]),
        ),
        ("Mood's median", AnalysisSpec::moods_median("C1", "G")),
        ("sign test", AnalysisSpec::sign_test("C1", 500.0)),
        ("Levene", AnalysisSpec::levene("C1", "G")),
        ("Bartlett", AnalysisSpec::bartlett("C1", "G")),
        (
            "chi-square independence",
            AnalysisSpec::chi_square_independence("G", "B"),
        ),
        ("two-way ANOVA", AnalysisSpec::two_way_anova("C1", "G", "B")),
        ("Grubbs", AnalysisSpec::grubbs("C1")),
        (
            "curve fit",
            AnalysisSpec::curve_fit("X", "Y", fp_core::curvefit::CurveModel::Polynomial(2)),
        ),
        ("interval plot", AnalysisSpec::interval_plot("C1", "G")),
        ("dotplot", AnalysisSpec::dotplot("C1")),
        (
            "matrix plot",
            AnalysisSpec::matrix_plot(vec!["X".to_owned(), "Y".to_owned(), "Z".to_owned()]),
        ),
        ("series plot", AnalysisSpec::series_plot("C1")),
        (
            "Hotelling T2",
            AnalysisSpec::hotelling_t2(vec!["X".to_owned(), "Y".to_owned()], 5),
        ),
        (
            "acceptance sampling",
            AnalysisSpec::acceptance_sampling(50, 2, Some(1_000)),
        ),
        (
            "exponential reliability",
            AnalysisSpec::exponential_reliability("T"),
        ),
        (
            "Weibull reliability",
            AnalysisSpec::weibull_reliability("T"),
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

/// The fits that iterate over the sample: ten maximum-likelihood families, the
/// Box-Cox profile search, a stepwise selection, the two generalised linear
/// models and an accelerated-life fit. Their cost grows with the number of
/// iterations, so the guard uses a hundred thousand rows and a wider budget;
/// what it protects is the shape of the curve, not the constant.
#[test]
fn iterative_fits_at_a_hundred_thousand_rows_stay_inside_the_budget() {
    let df = frame_with_rows(100_000);
    let limit = iterative_budget();
    let cases: Vec<(&str, AnalysisSpec)> = vec![
        (
            "distribution comparison",
            AnalysisSpec::distribution_comparison("C1"),
        ),
        ("Box-Cox transformation", AnalysisSpec::box_cox("T")),
        (
            "stepwise regression",
            AnalysisSpec::stepwise_regression(
                "Y",
                vec!["X".to_owned(), "Z".to_owned(), "W".to_owned()],
            ),
        ),
        (
            "ordinal logistic",
            AnalysisSpec::ordinal("O", vec!["X".to_owned()]),
        ),
        (
            "Poisson regression",
            AnalysisSpec::poisson("K", vec!["X".to_owned()]),
        ),
        ("accelerated life", AnalysisSpec::accelerated_life("T", "S")),
    ];
    for (name, spec) in cases {
        let started = Instant::now();
        let output = run_analysis(&df, &spec).unwrap_or_else(|error| panic!("{name}: {error}"));
        let elapsed = started.elapsed();
        println!("{name} (100k rows): {elapsed:?} (budget {limit:?})");
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
