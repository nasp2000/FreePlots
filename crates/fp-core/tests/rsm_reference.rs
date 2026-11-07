//! Independent reference values for the response surface (hand OLS + reference).
//!
//! The values in `tests/reference/rsm.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{Column, DataFrame};
use fp_core::rsm;
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/rsm.json");
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
    let rows = case["rows"].as_array().expect("rows");
    let mut x1 = Vec::new();
    let mut x2 = Vec::new();
    let mut y = Vec::new();
    for row in rows {
        let values = row.as_array().expect("row");
        x1.push(values[0].as_f64().expect("x1"));
        x2.push(values[1].as_f64().expect("x2"));
        y.push(values[2].as_f64().expect("y"));
    }
    DataFrame::new(vec![
        Column::new_f64("x1", x1),
        Column::new_f64("x2", x2),
        Column::new_f64("y", y),
    ])
    .expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance * (1.0 + expected.abs()),
        "{what}: {actual} vs oracle {expected}"
    );
}

#[test]
fn the_quadratic_surface_matches_the_oracle() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model =
        rsm::fit(&frame(&r), "y", &["x1".to_owned(), "x2".to_owned()]).expect("response surface");
    let terms: Vec<String> = r["terms"]
        .as_array()
        .expect("terms")
        .iter()
        .map(|value| value.as_str().expect("term").to_owned())
        .collect();
    assert_eq!(model.regression.names, terms);
    let coefficients = numbers(&r["coefficients"]);
    let standard_errors = numbers(&r["standard_errors"]);
    let t_values = numbers(&r["t"]);
    let p_values = numbers(&r["p"]);
    for index in 0..terms.len() {
        close(
            model.regression.coefficients[index],
            coefficients[index],
            tolerance,
            terms[index].as_str(),
        );
        close(
            model.regression.standard_errors[index],
            standard_errors[index],
            tolerance,
            "SE",
        );
        close(
            model.regression.t_values[index],
            t_values[index],
            tolerance,
            "t",
        );
        close(
            model.regression.p_values[index],
            p_values[index],
            tolerance,
            "p",
        );
    }
    close(
        model.regression.r_squared,
        r["r_squared"].as_f64().expect("R²"),
        tolerance,
        "R²",
    );
    close(
        model.regression.f,
        r["f"].as_f64().expect("F"),
        tolerance,
        "F",
    );
    close(
        model.regression.f_p_value,
        r["f_p"].as_f64().expect("F p"),
        tolerance,
        "F p",
    );

    let lack = model.lack_of_fit.expect("lack of fit");
    let expected = &r["lack_of_fit"];
    close(
        lack.ss,
        expected["ss"].as_f64().expect("lack SS"),
        tolerance,
        "lack SS",
    );
    assert_eq!(lack.df, expected["df"].as_f64().expect("lack df"));
    close(
        lack.f,
        expected["f"].as_f64().expect("lack F"),
        tolerance,
        "lack F",
    );
    close(
        lack.p_value,
        expected["p"].as_f64().expect("lack p"),
        tolerance,
        "lack p",
    );
}

#[test]
fn the_stationary_point_and_extrema_match_the_oracle() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let model =
        rsm::fit(&frame(&r), "y", &["x1".to_owned(), "x2".to_owned()]).expect("response surface");
    let optimization = rsm::optimize(&model).expect("optimize");
    let stationary = optimization.stationary.expect("stationary point");
    for (actual, expected) in stationary.iter().zip(numbers(&r["stationary"])) {
        close(*actual, expected, tolerance, "stationary");
    }
    close(
        optimization.predicted.expect("predicted"),
        r["predicted"].as_f64().expect("predicted"),
        tolerance,
        "predicted",
    );
    assert_eq!(optimization.nature, "maximum");
    for (actual, expected) in optimization
        .eigenvalues
        .iter()
        .zip(numbers(&r["eigenvalues"]))
    {
        close(*actual, expected, tolerance, "eigenvalue");
    }

    let minimum = optimization.minimum.expect("minimum");
    let expected = &r["minimum"];
    close(
        minimum.predicted,
        expected["value"].as_f64().expect("minimum"),
        tolerance,
        "minimum",
    );
    for (actual, expected) in minimum.point.iter().zip(numbers(&expected["point"])) {
        close(*actual, expected, tolerance, "minimum point");
    }
    let maximum = optimization.maximum.expect("maximum");
    let expected = &r["maximum"];
    close(
        maximum.predicted,
        expected["value"].as_f64().expect("maximum"),
        tolerance,
        "maximum",
    );
    for (actual, expected) in maximum.point.iter().zip(numbers(&expected["point"])) {
        close(*actual, expected, tolerance, "maximum point");
    }
}

#[test]
fn a_constant_response_is_rejected() {
    let r = reference();
    let mut df = frame(&r);
    for row in 0..df.n_rows() {
        df.set_cell(2, row, "10").expect("set cell");
    }
    assert!(rsm::fit(&df, "y", &["x1".to_owned(), "x2".to_owned()]).is_err());
}

#[test]
fn the_steepest_ascent_path_matches_the_closed_form() {
    // A noiseless quadratic: y = 10 + 2·A + 3·B − A² − B².
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for first in [-1.0, 0.0, 1.0] {
        for second in [-1.0, 0.0, 1.0] {
            a.push(first);
            b.push(second);
            y.push(10.0 + 2.0 * first + 3.0 * second - first * first - second * second);
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
    ])
    .expect("frame");
    let model = rsm::fit(&df, "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
    let ascent = rsm::steepest_ascent(&model, 0.5, 5).expect("path");
    assert_eq!(ascent.points.len(), 5);
    // The largest gradient component (B, 3) advances by the step 0.5.
    close(ascent.direction[0], 1.0 / 3.0, 1e-12, "A direction");
    close(ascent.direction[1], 0.5, 1e-12, "B direction");
    for (index, point) in ascent.points.iter().enumerate() {
        let step = (index + 1) as f64;
        let first = step / 3.0;
        let second = step / 2.0;
        close(point.coded[0], first, 1e-12, "A coded");
        close(point.coded[1], second, 1e-12, "B coded");
        close(
            point.predicted,
            10.0 + 2.0 * first + 3.0 * second - first * first - second * second,
            1e-9,
            "predicted",
        );
    }
}

#[test]
fn a_flat_surface_has_no_steepest_ascent() {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for first in [-1.0, 0.0, 1.0] {
        for second in [-1.0, 0.0, 1.0] {
            a.push(first);
            b.push(second);
            y.push(5.0 - first * first - second * second);
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
    ])
    .expect("frame");
    let model = rsm::fit(&df, "y", &["A".to_owned(), "B".to_owned()]).expect("fit");
    assert!(rsm::steepest_ascent(&model, 0.5, 5).is_err());
}
