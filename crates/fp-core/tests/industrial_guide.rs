// SPDX-License-Identifier: MIT
//! The industrial quick guide, end to end.
//!
//! One test per section of the guide, so a missing capability fails the suite:
//! descriptive statistics and charts, Cp/Cpk/Pp/Ppk, SPC (I-MR, X̄-R, P, U),
//! Gage R&R, hypothesis tests, ANOVA/correlation/regression, Pareto, DOE/RSM
//! and reliability.

use fp_core::attribute::AttributeChartKind;
use fp_core::data::{Column, DataFrame};
use fp_core::spc::ControlChartKind;
use fp_core::workflow::{AnalysisSpec, run_analysis};

fn text(df: &DataFrame, spec: &AnalysisSpec) -> String {
    let output = run_analysis(df, spec).unwrap_or_else(|error| panic!("{spec:?}: {error}"));
    output
        .results
        .iter()
        .map(|result| result.to_session_text())
        .collect::<Vec<_>>()
        .join("\n")
}

fn numbers(values: Vec<f64>) -> DataFrame {
    DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
}

fn dimensions() -> DataFrame {
    // 100 diameters around 25.00 mm, deterministic.
    let values = (0..100)
        .map(|index| 25.0 + 0.08 * ((index as f64 * 12.9898).sin()))
        .collect();
    numbers(values)
}

#[test]
fn descriptive_statistics_and_charts() {
    let df = dimensions();
    let text = text(&df, &AnalysisSpec::descriptive_statistics("x"));
    for label in [
        "Mean",
        "Median",
        "Standard deviation",
        "Minimum",
        "Maximum",
        "First quartile",
        "Third quartile",
    ] {
        assert!(text.contains(label), "{label} missing");
    }
    for spec in [AnalysisSpec::histogram("x"), AnalysisSpec::boxplot("x")] {
        let output = run_analysis(&df, &spec).expect("chart");
        assert!(output.plot.is_some(), "{spec:?} produced no plot");
    }
}

#[test]
fn capability_indices() {
    let df = dimensions();
    let output = run_analysis(&df, &AnalysisSpec::capability("x", Some(24.7), Some(25.3)))
        .expect("capability");
    let text = output.results[0].to_session_text();
    for label in ["Cp", "Cpk", "Pp", "Ppk", "PPM"] {
        assert!(text.contains(label), "{label} missing");
    }
}

#[test]
fn spc_charts() {
    // 20 subgroups of five.
    let mut group = Vec::new();
    let mut value = Vec::new();
    for g in 0..20 {
        for r in 0..5 {
            group.push((g + 1).to_string());
            value.push(10.0 + 0.4 * ((g + r) as f64).sin());
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("value", value),
        Column::new_text("group", group),
    ])
    .expect("frame");
    let imr = text(
        &df,
        &AnalysisSpec::control_chart("value", ControlChartKind::IndividualsMovingRange, 1),
    );
    for label in ["I–MR", "Center line", "Location LCL", "Location UCL"] {
        assert!(imr.contains(label), "{label} missing");
    }
    let xbar = text(
        &df,
        &AnalysisSpec::control_chart("value", ControlChartKind::XbarR, 5),
    );
    assert!(xbar.contains("X̄–R"), "Xbar-R missing");
    assert!(xbar.contains("Subgroups"), "subgroups missing");

    let defects: Vec<f64> = (0..20).map(|i| (i % 5) as f64).collect();
    let sizes: Vec<f64> = (0..20).map(|i| 100.0 + (i % 3) as f64).collect();
    let counts = DataFrame::new(vec![
        Column::new_f64("defects", defects),
        Column::new_f64("size", sizes),
    ])
    .expect("frame");
    let p = text(
        &counts,
        &AnalysisSpec::attribute_chart(AttributeChartKind::P, "defects")
            .with_attribute_size(Some("size".to_owned()), None),
    );
    assert!(p.contains("P Chart"), "P chart missing");
    let u = text(
        &counts,
        &AnalysisSpec::attribute_chart(AttributeChartKind::U, "defects")
            .with_attribute_size(Some("size".to_owned()), None),
    );
    assert!(u.contains("U Chart"), "U chart missing");
}

#[test]
fn msa_gage_rr() {
    // Ten parts, three operators, three replicates.
    let mut measure = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    for p in 1..=10 {
        let reference = 10.0 + 0.5 * f64::from(p);
        for (index, who) in ["A", "B", "C"].iter().enumerate() {
            for r in 0..3 {
                measure.push(reference + 0.02 * f64::from(r) + 0.01 * index as f64);
                part.push(format!("P{p}"));
                operator.push((*who).to_owned());
            }
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("measure", measure),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("frame");
    let text = text(&df, &AnalysisSpec::gage_rr("measure", "part", "operator"));
    for label in ["Repeatability", "Reproducibility", "% Contribution"] {
        assert!(text.contains(label), "{label} missing");
    }
}

#[test]
fn hypothesis_tests() {
    let df = {
        let a: Vec<f64> = (0..25).map(|i| 10.0 + (i as f64 * 0.7).sin()).collect();
        let b: Vec<f64> = (0..25).map(|i| 10.4 + (i as f64 * 0.7).cos()).collect();
        DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame")
    };
    let two = text(&df, &AnalysisSpec::two_sample_t("a", "b"));
    assert!(two.contains("2-Sample t"), "t test missing");
    assert!(two.contains("p-value"), "p-value missing");
    assert!(two.contains("CI"), "interval missing");

    let pass: Vec<f64> = (0..40).map(|i| f64::from(u8::from(i % 3 != 0))).collect();
    let prop = numbers(pass);
    let proportion = text(&prop, &AnalysisSpec::one_proportion("x", 0.5));
    assert!(proportion.contains("Blaker"), "exact method missing");
    assert!(proportion.contains("p-value"), "p-value missing");
}

#[test]
fn anova_correlation_and_regression() {
    let mut value = Vec::new();
    let mut line = Vec::new();
    for (name, shift) in [("A", 0.0), ("B", 1.2), ("C", 2.4)] {
        for i in 0..15 {
            value.push(10.0 + shift + 0.2 * f64::from(i % 4));
            line.push(name.to_owned());
        }
    }
    let anova_df = DataFrame::new(vec![
        Column::new_f64("value", value),
        Column::new_text("line", line),
    ])
    .expect("frame");
    let anova = text(&anova_df, &AnalysisSpec::anova("value", "line"));
    assert!(anova.contains("One-Way ANOVA"), "ANOVA missing");
    assert!(anova.contains('F'), "F statistic missing");

    let x: Vec<f64> = (0..30).map(|i| 150.0 + f64::from(i % 11)).collect();
    let z: Vec<f64> = (0..30).map(|i| 5.0 + 0.3 * f64::from(i % 7)).collect();
    let y: Vec<f64> = x
        .iter()
        .zip(&z)
        .map(|(x, z)| 20.0 + 0.05 * x + 2.0 * z)
        .collect();
    let regression_df = DataFrame::new(vec![
        Column::new_f64("x", x),
        Column::new_f64("z", z),
        Column::new_f64("y", y),
    ])
    .expect("frame");
    let correlation = text(
        &regression_df,
        &AnalysisSpec::correlation(vec!["x".to_owned(), "z".to_owned(), "y".to_owned()]),
    );
    assert!(correlation.contains("Correlation"), "correlation missing");
    let regression = text(
        &regression_df,
        &AnalysisSpec::regression("y", vec!["x".to_owned(), "z".to_owned()]),
    );
    assert!(regression.contains("R²"), "R² missing");
    assert!(
        regression.contains('x') && regression.contains('z'),
        "terms missing"
    );
}

#[test]
fn pareto_orders_the_defects() {
    let defects = ["scratch", "scratch", "scratch", "dent", "dent", "crack"];
    let df = DataFrame::new(vec![Column::new_text(
        "defect",
        vec!["scratch", "dent", "scratch", "crack", "scratch", "dent"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )])
    .expect("frame");
    assert_eq!(defects.len(), df.n_rows());
    let text = text(&df, &AnalysisSpec::pareto("defect"));
    assert!(text.contains("Pareto"), "Pareto missing");
    assert!(text.contains("Cumulative"), "cumulative missing");
}

#[test]
fn doe_interactions_and_optimization() {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for level_a in [-1.0, 1.0] {
        for level_b in [-1.0, 1.0] {
            for replicate in 0..3 {
                a.push(level_a);
                b.push(level_b);
                y.push(
                    50.0 + 4.0 * level_a
                        + 3.0 * level_b
                        + 2.0 * level_a * level_b
                        + 0.1 * f64::from(replicate),
                );
            }
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
    ])
    .expect("frame");
    let factorial = text(
        &df,
        &AnalysisSpec::factorial("y", vec!["A".to_owned(), "B".to_owned()]),
    );
    assert!(factorial.contains("A*B"), "interaction term missing");
    assert!(factorial.contains("R²"), "model summary missing");

    // A 3x3 grid supports the quadratic surface.
    let mut ra = Vec::new();
    let mut rb = Vec::new();
    let mut ry = Vec::new();
    for x in [-1.0, 0.0, 1.0] {
        for z in [-1.0, 0.0, 1.0] {
            ra.push(x);
            rb.push(z);
            ry.push(60.0 + 3.0 * x + 2.0 * z - 2.0 * x * x - 1.5 * z * z + x * z);
        }
    }
    let rsm_df = DataFrame::new(vec![
        Column::new_f64("A", ra),
        Column::new_f64("B", rb),
        Column::new_f64("y", ry),
    ])
    .expect("frame");
    let rsm = text(
        &rsm_df,
        &AnalysisSpec::response_surface("y", vec!["A".to_owned(), "B".to_owned()])
            .with_rsm_optimization(true),
    );
    assert!(rsm.contains("Response Surface"), "RSM missing");
    assert!(
        rsm.contains("Stationary") || rsm.contains("Optimum"),
        "optimum missing"
    );
}

#[test]
fn reliability_curve() {
    let times: Vec<f64> = (1..=20).map(f64::from).collect();
    let df = numbers(times);
    let text = text(&df, &AnalysisSpec::kaplan_meier("x"));
    assert!(text.contains("Kaplan"), "Kaplan-Meier missing");
    assert!(text.contains("Median"), "median missing");
}
