// SPDX-License-Identifier: MIT
//! Independent reference values for the one-way ANOVA and its multiple
//! comparisons.
//!
//! The values in `tests/reference/anova.json` were computed independently;
//! the test reads the committed file and never runs an external process.

use fp_core::anova;
use fp_core::data::{Column, DataFrame};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/anova.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs reference {expected}"
    );
}

fn frame(data: &Value) -> DataFrame {
    let object = data.as_object().expect("data object");
    let mut levels = Vec::new();
    let mut values = Vec::new();
    for (level, sample) in object {
        for value in sample.as_array().expect("sample") {
            levels.push(level.clone());
            values.push(value.as_f64().expect("value"));
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", values),
        Column::new_text("g", levels),
    ])
    .expect("frame")
}

#[test]
fn one_way_anova_matches_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    let confidence = r["confidence"].as_f64().expect("confidence");
    let model = anova::fit(&frame(&r["data"]), "y", "g", confidence).expect("anova");

    for (key, field) in [
        ("grand_mean", model.grand_mean),
        ("ss_between", model.ss_between),
        ("ss_within", model.ss_within),
        ("ss_total", model.ss_total),
        ("df_between", model.df_between),
        ("df_within", model.df_within),
        ("ms_between", model.ms_between),
        ("ms_within", model.ms_within),
        ("f", model.f),
        ("p_value", model.p_value),
        ("r_squared", model.r_squared),
        ("adjusted_r_squared", model.adjusted_r_squared),
        ("pooled_sd", model.pooled_sd),
    ] {
        close(
            field,
            r[key].as_f64().expect("reference value"),
            tolerance,
            key,
        );
    }

    let groups = r["groups"].as_object().expect("groups");
    assert_eq!(model.groups.len(), groups.len());
    for summary in &model.groups {
        let expected = &groups[&summary.level];
        close(
            summary.mean,
            expected["mean"].as_f64().expect("mean"),
            tolerance,
            "group mean",
        );
        close(
            summary.sd,
            expected["sd"].as_f64().expect("sd"),
            tolerance,
            "group sd",
        );
        assert_eq!(
            summary.n,
            expected["n"].as_u64().expect("n") as usize,
            "group n"
        );
        close(
            summary.ci_lower,
            expected["ci_lower"].as_f64().expect("lower"),
            tolerance,
            "group ci lower",
        );
        close(
            summary.ci_upper,
            expected["ci_upper"].as_f64().expect("upper"),
            tolerance,
            "group ci upper",
        );
    }
}

#[test]
fn the_pairwise_comparisons_match_reference() {
    let r = reference();
    let tolerance = r["tolerance"].as_f64().expect("tolerance");
    // The studentized range quantile is evaluated numerically, so the Tukey
    // and Games–Howell values carry a slightly wider tolerance.
    let range_tolerance = 1e-4;
    let confidence = r["confidence"].as_f64().expect("confidence");
    let model = anova::fit(&frame(&r["data"]), "y", "g", confidence).expect("anova");
    let expected = r["comparisons"].as_array().expect("comparisons");
    assert_eq!(model.pairwise.len(), expected.len());

    for (actual, expected) in model.pairwise.iter().zip(expected) {
        assert_eq!(actual.first, expected["first"].as_str().expect("first"));
        assert_eq!(actual.second, expected["second"].as_str().expect("second"));
        for (key, field) in [
            ("difference", actual.difference),
            ("se", actual.se),
            ("t", actual.t),
            ("p_raw", actual.p_raw),
            ("p_adjusted", actual.p_adjusted),
            ("ci_lower", actual.ci_lower),
            ("ci_upper", actual.ci_upper),
            ("lsd_lower", actual.lsd_lower),
            ("lsd_upper", actual.lsd_upper),
        ] {
            close(
                field,
                expected[key].as_f64().expect("reference value"),
                tolerance,
                key,
            );
        }
        for (key, field) in [
            ("q", actual.q),
            ("p_tukey", actual.p_tukey),
            ("tukey_lower", actual.tukey_lower),
            ("tukey_upper", actual.tukey_upper),
            ("gh_se", actual.gh_se),
            ("gh_df", actual.gh_df),
            ("gh_q", actual.gh_q),
            ("gh_p", actual.gh_p),
            ("gh_lower", actual.gh_lower),
            ("gh_upper", actual.gh_upper),
        ] {
            close(
                field,
                expected[key].as_f64().expect("reference value"),
                range_tolerance,
                key,
            );
        }
    }
}
