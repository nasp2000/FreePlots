// SPDX-License-Identifier: MIT
//! Independent reference values for k-means .
//!
//! The values in `tests/reference/kmeans.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::cluster;
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/kmeans.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn frame(case: &Value) -> DataFrame {
    let rows = case["values"].as_array().expect("values");
    let mut x = Vec::with_capacity(rows.len());
    let mut y = Vec::with_capacity(rows.len());
    for row in rows {
        let values = row.as_array().expect("row");
        x.push(values[0].as_f64().expect("x"));
        y.push(values[1].as_f64().expect("y"));
    }
    DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs reference {expected}"
    );
}

fn sorted_centres(centres: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let mut sorted = centres.to_vec();
    sorted.sort_by(|left, right| {
        left[0]
            .total_cmp(&right[0])
            .then_with(|| left[1].total_cmp(&right[1]))
    });
    sorted
}

#[test]
fn kmeans_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let k = r["k"].as_u64().expect("k") as usize;
    let model = cluster::fit(
        &frame(&r),
        &["x".to_owned(), "y".to_owned()],
        k,
        false,
        2026,
        100,
    )
    .expect("k-means");
    close(
        model.within_ss,
        r["within_ss"].as_f64().expect("within"),
        tolerance,
        "within SS",
    );
    close(
        model.between_ss,
        r["between_ss"].as_f64().expect("between"),
        tolerance,
        "between SS",
    );
    close(
        model.explained,
        r["explained"].as_f64().expect("explained"),
        tolerance,
        "explained",
    );

    let mut sizes = model.sizes.clone();
    sizes.sort_unstable();
    let expected: Vec<usize> = r["sizes"]
        .as_array()
        .expect("sizes")
        .iter()
        .map(|value| value.as_u64().expect("size") as usize)
        .collect();
    assert_eq!(sizes, expected);

    let actual = sorted_centres(&model.means);
    let expected: Vec<Vec<f64>> = r["centres"]
        .as_array()
        .expect("centres")
        .iter()
        .map(|row| {
            row.as_array()
                .expect("centre")
                .iter()
                .map(|value| value.as_f64().expect("coordinate"))
                .collect()
        })
        .collect();
    for (actual, expected) in actual.iter().zip(expected) {
        for (left, right) in actual.iter().zip(expected) {
            close(*left, right, tolerance, "centre");
        }
    }
}

#[test]
fn two_clusters_are_supported() {
    let r = reference();
    let model = cluster::fit(
        &frame(&r),
        &["x".to_owned(), "y".to_owned()],
        2,
        false,
        7,
        100,
    )
    .expect("two clusters");
    assert_eq!(model.sizes.len(), 2);
    assert_eq!(model.sizes.iter().sum::<usize>(), model.n);
    assert!(model.within_ss > 0.0);
}

#[test]
fn an_impossible_cluster_count_is_rejected() {
    let r = reference();
    assert!(
        cluster::fit(
            &frame(&r),
            &["x".to_owned(), "y".to_owned()],
            1,
            false,
            1,
            10
        )
        .is_err()
    );
    assert!(
        cluster::fit(
            &frame(&r),
            &["x".to_owned(), "y".to_owned()],
            100,
            false,
            1,
            10
        )
        .is_err()
    );
}
