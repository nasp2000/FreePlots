// SPDX-License-Identifier: MIT
//! Independent reference values for the dotplot, scatterplot, stem-and-leaf,
//! series plot, matrix plot, distribution-free tolerance interval and the
//! exponential, power and logistic curve fits.
//!
//! The values in `tests/reference/charts4.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::curvefit::{self, CurveModel};
use fp_core::data::{Column, DataFrame, MissingPolicy, Sample};
use fp_core::dotplot;
use fp_core::matrixplot;
use fp_core::scatter;
use fp_core::seriesplot;
use fp_core::stemleaf;
use fp_core::tolerance;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/charts4.json");
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

fn frame(x: &[f64], y: &[f64]) -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("x", x.to_vec()),
        Column::new_f64("y", y.to_vec()),
    ])
    .expect("frame")
}

#[test]
fn the_dotplot_groups_and_means_match_the_oracle() {
    let r = reference();
    let case = &r["dotplot"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let values = numbers(&case["values"]);
    let groups: Vec<String> = case["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|value| value.as_str().expect("group").to_owned())
        .collect();

    let ungrouped = DataFrame::new(vec![Column::new_f64("value", values.clone())]).expect("frame");
    let model = dotplot::fit(&ungrouped, "value", None).expect("dotplot");
    assert_eq!(model.n, values.len());
    close(
        model.means[0],
        case["ungrouped_mean"].as_f64().expect("mean"),
        tolerance,
        "mean",
    );

    let df = DataFrame::new(vec![
        Column::new_f64("value", values),
        Column::new_text("group", groups),
    ])
    .expect("frame");
    let model = dotplot::fit(&df, "value", Some("group")).expect("grouped dotplot");
    assert_eq!(model.levels, vec!["A".to_owned(), "B".to_owned()]);
    for (index, level) in model.levels.iter().enumerate() {
        close(
            model.means[index],
            case["group_means"][level].as_f64().expect("mean"),
            tolerance,
            "group mean",
        );
        assert_eq!(model.values[index].len(), 6);
    }
}

#[test]
fn the_scatterplot_correlations_match_reference() {
    let r = reference();
    let case = &r["scatter"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let x = numbers(&case["x"]);
    let y = numbers(&case["y"]);
    let df = frame(&x, &y);
    let model = scatter::fit(&df, "x", "y", true).expect("scatter");
    assert_eq!(model.x.len(), x.len());
    close(model.r, case["r"].as_f64().expect("r"), tolerance, "r");
    close(
        model.p_value,
        case["p"].as_f64().expect("p"),
        tolerance,
        "p",
    );
    close(
        model.slope,
        case["slope"].as_f64().expect("slope"),
        tolerance,
        "slope",
    );
    close(
        model.intercept,
        case["intercept"].as_f64().expect("intercept"),
        tolerance,
        "intercept",
    );
    let (rho, p) = scatter::spearman(&x, &y);
    close(
        rho,
        case["spearman_r"].as_f64().expect("rho"),
        tolerance,
        "rho",
    );
    close(
        p,
        case["spearman_p"].as_f64().expect("p"),
        tolerance,
        "rho p",
    );
    let (tau, _) = scatter::kendall(&x, &y);
    close(
        tau,
        case["kendall_tau"].as_f64().expect("tau"),
        tolerance,
        "tau-b",
    );
}

fn check_stem(case: &Value, tolerance: f64) {
    let values = numbers(&case["values"]);
    let display = stemleaf::from_values(&values).expect("stem and leaf");
    close(
        display.unit,
        case["unit"].as_f64().expect("unit"),
        tolerance,
        "unit",
    );
    let stems = case["stems"].as_array().expect("stems");
    assert_eq!(display.stems.len(), stems.len());
    for (actual, expected) in display.stems.iter().zip(stems) {
        close(
            actual.stem,
            expected["stem"].as_f64().expect("stem"),
            tolerance,
            "stem",
        );
        let leaves: Vec<u8> = expected["leaves"]
            .as_array()
            .expect("leaves")
            .iter()
            .map(|value| value.as_u64().expect("leaf") as u8)
            .collect();
        assert_eq!(actual.leaves, leaves);
        assert_eq!(
            actual.leaves.len(),
            expected["count"].as_u64().expect("count") as usize
        );
    }
    assert_eq!(display.n, values.len());
    assert_eq!(
        display
            .stems
            .iter()
            .map(|stem| stem.leaves.len())
            .sum::<usize>(),
        values.len()
    );
}

#[test]
fn the_stem_and_leaf_display_matches_the_oracle() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    check_stem(&r["stemleaf"], tolerance);
    check_stem(&r["stemleaf_negative"], tolerance);
}

#[test]
fn the_series_plot_uses_rows_and_a_time_column() {
    let r = reference();
    let case = &r["series"];
    let values = numbers(&case["values"]);
    let time = numbers(&case["time"]);
    let df = DataFrame::new(vec![
        Column::new_f64("value", values.clone()),
        Column::new_f64("time", time.clone()),
    ])
    .expect("frame");
    let model = seriesplot::fit(&df, "value", None).expect("series");
    assert_eq!(model.n, values.len());
    for (index, (x, y)) in model.x.iter().zip(&model.y).enumerate() {
        close(*x, (index + 1) as f64, 1e-12, "row index");
        close(*y, values[index], 1e-12, "value");
    }
    let model = seriesplot::fit(&df, "value", Some("time")).expect("series with time");
    assert_eq!(model.time.as_deref(), Some("time"));
    for (x, y) in model.x.iter().zip(&time) {
        close(*x, *y, 1e-12, "time");
    }
    assert!(seriesplot::fit(&df, "value", Some("value")).is_err());
}

#[test]
fn the_matrix_plot_builds_every_panel() {
    let r = reference();
    let case = &r["matrix"];
    let a = numbers(&case["a"]);
    let b = numbers(&case["b"]);
    let c = numbers(&case["c"]);
    let df = DataFrame::new(vec![
        Column::new_f64("a", a.clone()),
        Column::new_f64("b", b.clone()),
        Column::new_f64("c", c.clone()),
    ])
    .expect("frame");
    let columns = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
    let out = matrixplot::analyze(&df, &columns, 6).expect("matrix");
    let plot = out.plot.expect("plot");
    assert_eq!(plot.panel_columns, 3);
    assert_eq!(plot.panels.len(), 9);
    assert_eq!(plot.panels[0].bands.len(), 6);
    assert!(plot.panels[0].points.is_empty());
    // Panel (row b, column a) is the b-vs-a scatterplot.
    let panel = &plot.panels[3];
    assert_eq!(panel.title, "b vs a");
    assert_eq!(panel.points.len(), a.len());
    close(panel.points[0].x, a[0], 1e-12, "x");
    close(panel.points[0].y, b[0], 1e-12, "y");
    // Panel (row a, column b) is the transpose.
    let transpose = &plot.panels[1];
    close(transpose.points[0].x, b[0], 1e-12, "x");
    close(transpose.points[0].y, a[0], 1e-12, "y");
    let statistics = &out.results[0].statistics;
    let value = |name: &str| {
        statistics
            .iter()
            .find(|(label, _)| *label == name)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("no {name}"))
    };
    close(value("Variables"), 3.0, 1e-12, "variables");
    close(value("Panels"), 9.0, 1e-12, "panels");
    close(value("Observations"), a.len() as f64, 1e-12, "n");
    assert!(matrixplot::analyze(&df, &["a".to_owned()], 6).is_err());
    assert!(
        matrixplot::analyze(&df, &["a".to_owned(), "a".to_owned()], 6).is_err(),
        "duplicate variables are rejected"
    );
}

#[test]
fn the_distribution_free_interval_matches_the_beta_coverage() {
    let r = reference();
    let case = &r["free_tolerance"];
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let proportion = case["proportion"].as_f64().expect("proportion");
    let confidence = case["confidence"].as_f64().expect("confidence");
    let sample = Sample::new(numbers(&case["values"]), MissingPolicy::Drop).expect("sample");
    let interval = tolerance::distribution_free(&sample, proportion, confidence).expect("interval");
    let expected = &case["best"];
    assert_eq!(
        interval.order,
        expected["order"].as_u64().expect("order") as usize
    );
    close(
        interval.low,
        expected["low"].as_f64().expect("low"),
        tolerance,
        "low",
    );
    close(
        interval.high,
        expected["high"].as_f64().expect("high"),
        tolerance,
        "high",
    );
    close(
        interval.achieved,
        expected["achieved"].as_f64().expect("achieved"),
        tolerance,
        "achieved",
    );
    let large = Sample::new(numbers(&case["large_values"]), MissingPolicy::Drop).expect("sample");
    let interval = tolerance::distribution_free(&large, proportion, confidence).expect("interval");
    let expected = &case["large_best"];
    assert_eq!(
        interval.order,
        expected["order"].as_u64().expect("order") as usize
    );
    close(
        interval.low,
        expected["low"].as_f64().expect("low"),
        tolerance,
        "large low",
    );
    close(
        interval.high,
        expected["high"].as_f64().expect("high"),
        tolerance,
        "large high",
    );
    assert_eq!(
        tolerance::minimum_sample(proportion, confidence),
        Some(case["minimum_sample"].as_u64().expect("minimum") as usize)
    );
    assert_eq!(
        tolerance::minimum_sample(0.95, 0.95),
        Some(case["minimum_sample_95"].as_u64().expect("minimum") as usize)
    );
    assert!(tolerance::distribution_free(&sample, 1.5, 0.95).is_err());
}

#[test]
fn the_exponential_and_power_fits_match_the_log_scale_regression() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    for (model, case) in [
        (CurveModel::Exponential, &r["exponential"]),
        (CurveModel::Power, &r["power"]),
    ] {
        let df = frame(&numbers(&case["x"]), &numbers(&case["y"]));
        let fit = curvefit::fit(&df, "x", "y", model, 0.95).expect("curve fit");
        close(
            fit.coefficients[0],
            case["intercept"].as_f64().expect("intercept"),
            tolerance,
            "log intercept",
        );
        close(
            fit.coefficients[1],
            case["slope"].as_f64().expect("slope"),
            tolerance,
            "slope",
        );
        assert!(fit.r_squared > 0.95, "{:?}: {}", model, fit.r_squared);
    }
    // A non-positive response cannot be log-transformed.
    let df = frame(
        &numbers(&r["exponential"]["x"]),
        &[1.0, 0.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
    );
    assert!(curvefit::fit(&df, "x", "y", CurveModel::Exponential, 0.95).is_err());
    // The power model needs positive x as well.
    let df = frame(&[0.0, 1.0, 2.0, 3.0], &[1.0, 2.0, 3.0, 4.0]);
    assert!(curvefit::fit(&df, "x", "y", CurveModel::Power, 0.95).is_err());
}

#[test]
fn the_logistic_fit_recovers_the_parameters() {
    let r = reference();
    let case = &r["logistic"];
    let df = frame(&numbers(&case["x"]), &numbers(&case["y"]));
    let fit = curvefit::fit(&df, "x", "y", CurveModel::Logistic, 0.95).expect("logistic");
    assert_eq!(fit.coefficients.len(), 4);
    for (actual, expected) in fit.coefficients.iter().zip(numbers(&case["parameters"])) {
        close(*actual, expected, 1e-3, "logistic parameter");
    }
    assert!(fit.r_squared > 0.99, "{}", fit.r_squared);
}
