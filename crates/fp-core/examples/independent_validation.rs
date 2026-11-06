//! Independent statistical validation for FreePlots.
//!
//! The checks use deterministic samples and expected values taken from
//! published definitions, standard tables or exact algebra. Nothing here
//! depends on a private fixture, a commercial package or a documented file, so
//! the same command runs in a clean public checkout:
//!
//! ```text
//! cargo run --locked -p fp-core --example independent_validation
//! ```
//!
//! Each section covers a different part of the engine. The process exits with
//! a non-zero status on the first failed check.

use fp_core::data::{Column, DataFrame};
use fp_core::dists::{Distribution, Exponential};
use fp_core::hypothesis::{self, Alternative};
use fp_core::regression;
use fp_core::special;
use fp_core::workflow;

fn frame(columns: Vec<Column>) -> DataFrame {
    DataFrame::new(columns).expect("the validation frame is well formed")
}

fn numbers(column: &str, values: Vec<f64>) -> DataFrame {
    frame(vec![Column::new_f64(column, values)])
}

fn stat(result: &fp_core::report::AnalysisResult, name: &str) -> f64 {
    result
        .statistics
        .iter()
        .find(|(label, _)| *label == name)
        .map(|(_, value)| *value)
        .unwrap_or_else(|| {
            panic!(
                "{} is missing from {:?}",
                name,
                result
                    .statistics
                    .iter()
                    .map(|(label, _)| *label)
                    .collect::<Vec<_>>()
            )
        })
}

fn stat_starting(result: &fp_core::report::AnalysisResult, prefix: &str) -> f64 {
    result
        .statistics
        .iter()
        .find(|(label, _)| label.starts_with(prefix))
        .map(|(_, value)| *value)
        .unwrap_or_else(|| panic!("{prefix} is missing from the descriptive statistics"))
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected}, got {actual} (tolerance {tolerance})"
    );
    println!("ok   {what}: {actual}");
}

fn exact(actual: f64, expected: f64, what: &str) {
    close(actual, expected, 1e-10, what);
}

fn main() {
    descriptive_statistics();
    normal_distribution();
    student_t_distribution();
    exponential_distribution();
    one_sample_t();
    two_sample_t();
    one_way_anova();
    exact_regression();
    println!("\nindependent validation passed");
}

fn descriptive_statistics() {
    let df = numbers("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let result = workflow::descriptive_statistics(&df, "x").expect("descriptive statistics");
    assert_eq!(result.n, 5, "descriptive n");
    close(stat(&result, "Mean"), 3.0, 1e-12, "descriptive mean");
    close(
        stat_starting(&result, "Standard deviation"),
        2.5_f64.sqrt(),
        1e-12,
        "descriptive n-1 standard deviation",
    );
    close(stat(&result, "Median"), 3.0, 1e-12, "descriptive median");
}

fn normal_distribution() {
    // The median and the 97.5 % point are standard normal constants.
    exact(special::normal_cdf(0.0), 0.5, "standard normal CDF at 0");
    close(
        special::normal_cdf(1.959_963_984_540_054),
        0.975,
        1e-12,
        "standard normal CDF at 1.959963984540054",
    );
    close(
        special::normal_quantile(0.975),
        1.959_963_984_540_054,
        1e-12,
        "standard normal 97.5 % quantile",
    );
    exact(special::normal_quantile(0.5), 0.0, "standard normal median");
}

fn student_t_distribution() {
    // Published two-sided 95 % critical value for eight degrees of freedom.
    close(
        special::student_t_quantile(0.975, 8.0),
        2.306_004_135_204_166,
        1e-9,
        "Student t 97.5 % quantile, df = 8",
    );
    exact(
        special::student_t_cdf(0.0, 8.0),
        0.5,
        "Student t CDF at 0, df = 8",
    );
}

fn exponential_distribution() {
    let distribution = Exponential::new(2.0).expect("exponential scale 2");
    // F(mean) = 1 - exp(-1), a closed form independent of the implementation.
    close(
        distribution.cdf(2.0),
        1.0 - (-1.0_f64).exp(),
        1e-15,
        "exponential CDF at the mean",
    );
    exact(distribution.cdf(0.0), 0.0, "exponential CDF at 0");
}

fn one_sample_t() {
    let df = numbers("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    let result =
        hypothesis::one_sample(&df, "x", 0.0, Alternative::TwoSided, 0.95).expect("one-sample t");
    close(
        stat(&result, "t"),
        4.242_640_687_119_285,
        1e-12,
        "one-sample t statistic",
    );
    close(stat(&result, "df"), 4.0, 1e-12, "one-sample t df");
    // Two-sided p for t = 4.242640687, df = 4, from standard t tables.
    close(
        stat(&result, "p-value"),
        0.013_2,
        1e-4,
        "one-sample t p-value",
    );
}

fn two_sample_t() {
    let df = frame(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("b", vec![2.0, 3.0, 4.0, 5.0, 6.0]),
    ]);
    let result = hypothesis::two_sample(&df, "a", "b", true, Alternative::TwoSided, 0.95)
        .expect("pooled two-sample t");
    close(
        stat(&result, "t"),
        -1.0,
        1e-12,
        "pooled two-sample t statistic",
    );
    close(stat(&result, "df"), 8.0, 1e-12, "pooled two-sample t df");
    close(
        stat(&result, "p-value"),
        0.346_6,
        1e-4,
        "pooled two-sample t p-value",
    );
}

fn one_way_anova() {
    let df = frame(vec![
        Column::new_f64("value", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]),
        Column::new_text(
            "group",
            vec![
                "A".to_string(),
                "A".to_string(),
                "A".to_string(),
                "B".to_string(),
                "B".to_string(),
                "B".to_string(),
                "C".to_string(),
                "C".to_string(),
                "C".to_string(),
            ],
        ),
    ]);
    let fit = fp_core::anova::fit(&df, "value", "group", 0.95).expect("one-way ANOVA");
    assert_eq!(fit.n, 9, "ANOVA n");
    close(fit.ss_between, 54.0, 1e-12, "ANOVA between SS");
    close(fit.ss_within, 6.0, 1e-12, "ANOVA within SS");
    close(fit.f, 27.0, 1e-12, "ANOVA F statistic");
    assert!(
        (0.0..0.01).contains(&fit.p_value),
        "ANOVA p should be below 0.01, got {}",
        fit.p_value
    );
    println!("ok   ANOVA p-value: {} (< 0.01)", fit.p_value);
}

fn exact_regression() {
    let x1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let x2 = vec![2.0, 1.0, 4.0, 3.0, 5.0];
    let y: Vec<f64> = x1
        .iter()
        .zip(&x2)
        .map(|(a, b)| 5.0 + 2.0 * a + 3.0 * b)
        .collect();
    let df = frame(vec![
        Column::new_f64("y", y),
        Column::new_f64("x1", x1),
        Column::new_f64("x2", x2),
    ]);
    let model = regression::fit(&df, "y", &["x1".to_owned(), "x2".to_owned()], 0.95)
        .expect("exact regression");
    close(model.coefficients[0], 5.0, 1e-12, "regression constant");
    close(
        model.coefficients[1],
        2.0,
        1e-12,
        "regression x1 coefficient",
    );
    close(
        model.coefficients[2],
        3.0,
        1e-12,
        "regression x2 coefficient",
    );
    close(model.r_squared, 1.0, 1e-12, "regression R squared");
    for residual in &model.residuals {
        close(*residual, 0.0, 1e-12, "regression residual");
    }
}
