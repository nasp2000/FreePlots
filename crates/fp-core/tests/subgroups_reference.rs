// SPDX-License-Identifier: MIT
//! Independent reference values for the variable-size X̄–R and X̄–S charts.
//!
//! The values in `tests/reference/subgroups.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::data::{MissingPolicy, Sample};
use fp_core::spc::{self, ControlChartKind, SignalChart, TestSet};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/subgroups.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn samples(groups: &Value) -> Vec<Sample> {
    groups
        .as_array()
        .expect("groups")
        .iter()
        .map(|group| {
            Sample::new(
                group
                    .as_array()
                    .expect("group")
                    .iter()
                    .map(|value| value.as_f64().expect("value"))
                    .collect(),
                MissingPolicy::Drop,
            )
            .expect("sample")
        })
        .collect()
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs oracle {expected}"
    );
}

fn check(case: &Value, kind: ControlChartKind) {
    let tolerance = reference()["tolerance"].as_f64().expect("tolerance");
    let chart = spc::variable_subgroup_chart(&samples(&case["groups"]), kind, TestSet::default())
        .expect("chart");
    let expected_means = case["means"].as_array().expect("means");
    let expected_spreads = case["spreads"].as_array().expect("spreads");
    let expected_sizes = case["sizes"].as_array().expect("sizes");
    assert_eq!(chart.location.len(), expected_means.len());
    for (index, value) in expected_means.iter().enumerate() {
        close(
            chart.location[index],
            value.as_f64().expect("mean"),
            tolerance,
            "mean",
        );
    }
    for (index, value) in expected_spreads.iter().enumerate() {
        close(
            chart.dispersion[index],
            value.as_f64().expect("spread"),
            tolerance,
            "spread",
        );
    }
    for (index, value) in expected_sizes.iter().enumerate() {
        assert_eq!(
            chart.subgroup_sizes[index],
            value.as_u64().expect("size") as usize
        );
    }
    close(
        chart.sigma,
        case["sigma"].as_f64().expect("sigma"),
        tolerance,
        "sigma",
    );
    close(
        chart.location_limits.center,
        case["center"].as_f64().expect("center"),
        tolerance,
        "center",
    );
    for (index, expected) in case["location_steps"]
        .as_array()
        .expect("location steps")
        .iter()
        .enumerate()
    {
        let limits = chart.location_steps[index];
        close(
            limits.lcl,
            expected[0].as_f64().expect("lcl"),
            tolerance,
            "location lcl",
        );
        close(
            limits.center,
            expected[1].as_f64().expect("center"),
            tolerance,
            "location center",
        );
        close(
            limits.ucl,
            expected[2].as_f64().expect("ucl"),
            tolerance,
            "location ucl",
        );
    }
    for (index, expected) in case["dispersion_steps"]
        .as_array()
        .expect("dispersion steps")
        .iter()
        .enumerate()
    {
        let limits = chart.dispersion_steps[index];
        close(
            limits.lcl,
            expected[0].as_f64().expect("lcl"),
            tolerance,
            "dispersion lcl",
        );
        close(
            limits.center,
            expected[1].as_f64().expect("center"),
            tolerance,
            "dispersion center",
        );
        close(
            limits.ucl,
            expected[2].as_f64().expect("ucl"),
            tolerance,
            "dispersion ucl",
        );
    }
}

#[test]
fn the_variable_xbar_r_chart_matches_the_oracle() {
    check(&reference()["r"], ControlChartKind::XbarR);
}

#[test]
fn the_variable_xbar_s_chart_matches_the_oracle() {
    check(&reference()["s"], ControlChartKind::XbarS);
}

#[test]
fn a_shifted_subgroup_is_flagged_against_its_own_limit() {
    let r = reference();
    let case = &r["outlier"];
    let chart = spc::variable_subgroup_chart(
        &samples(&case["groups"]),
        ControlChartKind::XbarR,
        TestSet::default(),
    )
    .expect("chart");
    assert!(
        chart.signals.iter().any(|signal| {
            signal.test == 1 && signal.chart == SignalChart::Location && signal.index == 2
        }),
        "{:?}",
        chart.signals
    );
}

#[test]
fn invalid_subgroups_are_rejected() {
    let r = reference();
    let groups = samples(&r["r"]["groups"]);
    assert!(
        spc::variable_subgroup_chart(&groups[..1], ControlChartKind::XbarR, TestSet::default())
            .is_err()
    );
    assert!(
        spc::variable_subgroup_chart(
            &groups,
            ControlChartKind::IndividualsMovingRange,
            TestSet::default()
        )
        .is_err()
    );
    let single = Sample::new(vec![10.0], MissingPolicy::Drop).expect("sample");
    assert!(
        spc::variable_subgroup_chart(
            &[groups[0].clone(), single],
            ControlChartKind::XbarR,
            TestSet::default()
        )
        .is_err()
    );
}
