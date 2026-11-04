//! Option tests for the dialogs: every control is exercised through the
//! accessible labels, with a typical, a boundary and an invalid variation.

use fp_automation::Automation;
use fp_core::data::{Column, DataFrame};

fn frame() -> DataFrame {
    let n = 24;
    let noise = |index: usize| ((index * 37 % 17) as f64 - 8.0) * 0.03;
    let weight: Vec<f64> = (0..n)
        .map(|index| 20.0 + 0.15 * (10.0 + index as f64 * 0.1) + noise(index))
        .collect();
    let height: Vec<f64> = (0..n).map(|index| 10.0 + index as f64 * 0.1).collect();
    let pressure: Vec<f64> = (0..n)
        .map(|index| 100.0 + (index % 6) as f64 * 2.0)
        .collect();
    let batch: Vec<String> = (0..n)
        .map(|index| if index % 2 == 0 { "A" } else { "B" }.to_owned())
        .collect();
    let line: Vec<String> = (0..n)
        .map(|index| if (index / 2) % 2 == 0 { "L1" } else { "L2" }.to_owned())
        .collect();
    let pass: Vec<f64> = (0..n).map(|index| (index % 2) as f64).collect();
    let defects: Vec<f64> = (0..n).map(|index| (index % 4) as f64).collect();
    let part: Vec<String> = (0..n).map(|index| format!("P{}", index % 6 + 1)).collect();
    let operator: Vec<String> = (0..n)
        .map(|index| if (index / 3) % 2 == 0 { "O1" } else { "O2" }.to_owned())
        .collect();
    let grade: Vec<f64> = (0..n).map(|index| (index % 3 + 1) as f64).collect();
    let pass2: Vec<f64> = (0..n).map(|index| ((index + 1) % 2) as f64).collect();
    DataFrame::new(vec![
        Column::new_f64("weight", weight),
        Column::new_f64("height", height),
        Column::new_f64("pressure", pressure),
        Column::new_f64("pass", pass),
        Column::new_f64("defects", defects),
        Column::new_f64("grade", grade),
        Column::new_f64("pass2", pass2),
        Column::new_text("batch", batch),
        Column::new_text("line", line),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("frame")
}

fn counts() -> DataFrame {
    // The goodness-of-fit reads one observed frequency per distinct value.
    DataFrame::new(vec![Column::new_f64(
        "observed",
        vec![10.0, 12.0, 8.0, 10.0],
    )])
    .expect("counts")
}

/// A small, non-collinear model with one informative predictor and one noise
/// predictor, for the regression dialogs.
fn model_frame() -> DataFrame {
    let x1: Vec<f64> = (0..20).map(|i| ((i * 37 % 17) as f64) / 3.0).collect();
    let x2: Vec<f64> = (0..20).map(|i| ((i * 53 % 19) as f64) / 5.0).collect();
    let y: Vec<f64> = (0..20)
        .map(|i| 3.0 + 2.0 * x1[i] - x2[i] + ((i * 7 % 5) as f64) * 0.01)
        .collect();
    let group: Vec<String> = (0..20)
        .map(|i| if i % 2 == 0 { "A" } else { "B" }.to_owned())
        .collect();
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_f64("x1", x1),
        Column::new_f64("x2", x2),
        Column::new_text("group", group),
    ])
    .expect("model frame")
}

fn tiny() -> DataFrame {
    DataFrame::new(vec![Column::new_f64("value", vec![1.0, 2.0])]).expect("tiny")
}

fn open(ui: &mut Automation, path: &[&str]) {
    ui.open_menu(path)
        .unwrap_or_else(|error| panic!("open {path:?}: {error}"));
}

#[test]
fn the_one_sample_t_dialog_uses_the_alternative_and_the_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1-Sample t…"]);
    ui.set_text("Null mean:", "20.5").expect("null mean");
    ui.select("Alternative:", "greater").expect("alternative");
    ui.set_text("Confidence:", "90").expect("confidence");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("1-Sample t"), "{session}");
    assert!(session.contains("90.0%"), "{session}");
    assert!(session.contains("greater"), "{session}");
}

#[test]
fn the_one_sample_t_dialog_accepts_the_widest_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1-Sample t…"]);
    ui.set_text("Confidence:", "99.9").expect("confidence");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("99.9%"), "{}", ui.session());
}

#[test]
fn the_one_sample_t_dialog_rejects_a_non_numeric_null_mean() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1-Sample t…"]);
    ui.set_text("Null mean:", "abc").expect("null mean");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be a number"), "{}", ui.status());
    assert!(!ui.session().contains("1-Sample t"), "{}", ui.session());
}

#[test]
fn the_two_sample_t_dialog_pools_the_variances_on_request() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2-Sample t…"]);
    ui.select("Second column:", "height").expect("second");
    ui.click("Assume equal variances (pooled)").expect("pooled");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("2-Sample t"), "{session}");
    assert!(session.to_lowercase().contains("pooled"), "{session}");
}

#[test]
fn the_two_sample_t_dialog_reads_the_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2-Sample t…"]);
    ui.select("Second column:", "height").expect("second");
    ui.select("Alternative:", "less").expect("alternative");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("less alternative"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_two_sample_t_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2-Sample t…"]);
    ui.select("Second column:", "weight").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_paired_t_dialog_runs_on_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Paired t…"]);
    ui.select("Second column:", "height").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Paired t"), "{}", ui.session());
}

#[test]
fn the_paired_t_dialog_accepts_a_narrow_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Paired t…"]);
    ui.select("Second column:", "height").expect("second");
    ui.set_text("Confidence:", "50").expect("confidence");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("50.0%"), "{}", ui.session());
}

#[test]
fn the_paired_t_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Paired t…"]);
    ui.select("Second column:", "weight").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_equivalence_dialog_reads_the_reference_and_the_margin() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Equivalence Test…"]);
    ui.set_text("Reference mean:", "20.5").expect("reference");
    ui.set_text("Margin:", "0.5").expect("margin");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Equivalence"), "{session}");
    assert!(session.contains("0.5"), "{session}");
}

#[test]
fn the_equivalence_dialog_rejects_a_zero_margin() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Equivalence Test…"]);
    ui.set_text("Margin:", "0").expect("margin");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("positive"), "{}", ui.status());
}

#[test]
fn the_equivalence_dialog_rejects_a_non_numeric_margin() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Equivalence Test…"]);
    ui.set_text("Margin:", "wide").expect("margin");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be a number"), "{}", ui.status());
}

#[test]
fn the_goodness_of_fit_dialog_compares_the_observed_and_expected_counts() {
    let mut ui = Automation::new();
    ui.load_frame(counts(), "counts.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Chi-Square Goodness-of-Fit…"],
    );
    ui.set_text("Expected counts:", "10,10,10,10")
        .expect("expected");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Goodness-of-Fit"), "{session}");
    assert!(session.contains("Chi-Square"), "{session}");
}

#[test]
fn the_goodness_of_fit_dialog_counts_the_estimated_parameters() {
    let mut ui = Automation::new();
    ui.load_frame(counts(), "counts.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Chi-Square Goodness-of-Fit…"],
    );
    ui.set_text("Expected counts:", "10,10,10,10")
        .expect("expected");
    ui.set_text("Estimated parameters:", "1")
        .expect("parameters");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("df"), "{session}");
    assert!(session.contains("2.000000"), "{session}");
}

#[test]
fn the_goodness_of_fit_dialog_asks_for_at_least_two_expected_counts() {
    let mut ui = Automation::new();
    ui.load_frame(counts(), "counts.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Chi-Square Goodness-of-Fit…"],
    );
    ui.set_text("Expected counts:", "10").expect("expected");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("comma-separated"), "{}", ui.status());
}

#[test]
fn the_grubbs_dialog_runs_on_a_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Grubbs Test…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Grubbs"), "{session}");
}

#[test]
fn the_grubbs_dialog_reads_the_significance() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Grubbs Test…"]);
    ui.set_text("Significance:", "1").expect("significance");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("1% level"), "{}", ui.session());
}

#[test]
fn the_grubbs_dialog_needs_at_least_three_observations() {
    let mut ui = Automation::new();
    ui.load_frame(tiny(), "tiny.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Grubbs Test…"]);
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("Grubbs"), "{}", ui.session());
}

#[test]
fn the_power_dialog_solves_for_the_sample_size() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Power and Sample Size…"],
    );
    ui.set_text("Effect size:", "0.5").expect("effect");
    ui.set_text("Sample size:", "0").expect("sample size");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Power and Sample Size"), "{session}");
    assert!(session.contains("Sample size"), "{session}");
}

#[test]
fn the_power_dialog_reads_the_significance_and_the_power() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Power and Sample Size…"],
    );
    ui.set_text("Sample size:", "20").expect("sample size");
    ui.set_text("Significance:", "0.1").expect("significance");
    ui.set_text("Power:", "99").expect("power");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("alpha 0.0010"), "{session}");
    assert!(
        session.contains("Requested power       0.990000"),
        "{session}"
    );
}

#[test]
fn the_power_dialog_rejects_a_non_numeric_effect() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Power and Sample Size…"],
    );
    ui.set_text("Effect size:", "large").expect("effect");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be a number"), "{}", ui.status());
}

#[test]
fn the_mann_whitney_dialog_reads_the_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Mann-Whitney…"]);
    ui.select("Second column:", "height").expect("second");
    ui.select("Alternative:", "greater").expect("alternative");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Mann-Whitney"), "{session}");
    assert!(session.contains("greater"), "{session}");
}

#[test]
fn the_mann_whitney_dialog_accepts_the_two_sided_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Mann-Whitney…"]);
    ui.select("Second column:", "pressure").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("two-sided"), "{}", ui.session());
}

#[test]
fn the_mann_whitney_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Mann-Whitney…"]);
    ui.select("Second column:", "weight").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_wilcoxon_dialog_reads_the_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Wilcoxon Signed-Rank…"],
    );
    ui.select("Second column:", "height").expect("second");
    ui.select("Alternative:", "less").expect("alternative");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Wilcoxon"), "{session}");
    assert!(session.contains("less"), "{session}");
}

#[test]
fn the_wilcoxon_dialog_accepts_the_two_sided_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Wilcoxon Signed-Rank…"],
    );
    ui.select("Second column:", "pressure").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("two-sided"), "{}", ui.session());
}

#[test]
fn the_wilcoxon_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Wilcoxon Signed-Rank…"],
    );
    ui.select("Second column:", "weight").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_kruskal_wallis_dialog_reads_the_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Kruskal-Wallis…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Kruskal-Wallis"), "{session}");
    assert!(session.contains("batch"), "{session}");
}

#[test]
fn the_kruskal_wallis_dialog_accepts_three_levels() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Kruskal-Wallis…"]);
    ui.select("Factor:", "part").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Kruskal-Wallis"), "{}", ui.session());
}

#[test]
fn the_kruskal_wallis_dialog_rejects_a_continuous_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Kruskal-Wallis…"]);
    ui.select("Factor:", "height").expect("factor");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("Kruskal-Wallis"), "{}", ui.session());
}

#[test]
fn the_correlation_matrix_dialog_correlates_every_pair() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Correlation Matrix…"],
    );
    ui.set_text("Columns:", "weight,height,pressure")
        .expect("columns");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Correlation"), "{session}");
    assert!(session.contains("pressure"), "{session}");
}

#[test]
fn the_correlation_matrix_dialog_accepts_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Correlation Matrix…"],
    );
    ui.set_text("Columns:", "weight,height").expect("columns");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("height"), "{}", ui.session());
}

#[test]
fn the_correlation_matrix_dialog_rejects_a_single_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Correlation Matrix…"],
    );
    ui.set_text("Columns:", "weight").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_tolerance_interval_dialog_runs_the_normal_limits() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Tolerance Interval…"],
    );
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Tolerance Interval"), "{session}");
    assert!(session.contains("Normal"), "{session}");
}

#[test]
fn the_tolerance_interval_dialog_runs_the_distribution_free_limits() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Tolerance Interval…"],
    );
    ui.click("Distribution-free").expect("method");
    ui.set_text("Proportion of the population:", "50")
        .expect("proportion");
    ui.set_text("Confidence:", "50").expect("confidence");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Distribution-Free"), "{session}");
    assert!(session.contains("50.0% of the population"), "{session}");
}

#[test]
fn the_tolerance_interval_dialog_rejects_a_constant_column() {
    let mut ui = Automation::new();
    let constant = DataFrame::new(vec![Column::new_f64("flat", vec![5.0; 10])]).expect("frame");
    ui.load_frame(constant, "constant.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Tolerance Interval…"],
    );
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(
        !ui.session().contains("Tolerance Interval"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_box_cox_dialog_transforms_a_positive_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Box-Cox Transformation…"],
    );
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Box-Cox"), "{session}");
    assert!(
        session.contains("lambda") || session.contains("λ"),
        "{session}"
    );
}

#[test]
fn the_box_cox_dialog_reports_the_before_and_after_normality() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Box-Cox Transformation…"],
    );
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("before"), "{session}");
    assert!(session.contains("after"), "{session}");
}

#[test]
fn the_box_cox_dialog_rejects_a_column_with_non_positive_values() {
    let mut ui = Automation::new();
    let signed = DataFrame::new(vec![Column::new_f64(
        "signed",
        vec![-2.0, -1.0, 0.0, 1.0, 2.0, 3.0],
    )])
    .expect("frame");
    ui.load_frame(signed, "signed.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Box-Cox Transformation…"],
    );
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("Box-Cox"), "{}", ui.session());
}

#[test]
fn the_partial_correlation_dialog_controls_for_the_third_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Partial Correlation…"],
    );
    ui.select("Y:", "height").expect("Y");
    ui.select("Control:", "pressure").expect("control");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Partial"), "{session}");
    assert!(session.contains("pressure"), "{session}");
}

#[test]
fn the_partial_correlation_dialog_accepts_every_numeric_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Partial Correlation…"],
    );
    ui.select("Y:", "defects").expect("Y");
    ui.select("Control:", "grade").expect("control");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Partial"), "{}", ui.session());
}

#[test]
fn the_partial_correlation_dialog_rejects_repeated_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Partial Correlation…"],
    );
    ui.select("Y:", "weight").expect("Y");
    ui.select("Control:", "pressure").expect("control");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("three different columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_friedman_dialog_ranks_three_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Friedman…"]);
    ui.set_text("More columns:", "height,pressure")
        .expect("columns");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Friedman"), "{session}");
    assert!(session.contains("pressure"), "{session}");
}

#[test]
fn the_friedman_dialog_accepts_one_more_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Friedman…"]);
    ui.set_text("More columns:", "height").expect("columns");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Friedman"), "{}", ui.session());
}

#[test]
fn the_friedman_dialog_needs_more_than_one_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Friedman…"]);
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least one more column"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_sign_test_dialog_uses_the_hypothesised_median() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Sign Test…"]);
    ui.set_text("Hypothesised median:", "20.5").expect("median");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Sign Test"), "{session}");
    assert!(session.contains("20.5"), "{session}");
}

#[test]
fn the_sign_test_dialog_defaults_to_the_sample_median() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Sign Test…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Sign Test"), "{session}");
    assert!(session.contains("median"), "{session}");
}

#[test]
fn the_sign_test_dialog_rejects_a_non_numeric_median() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Sign Test…"]);
    ui.set_text("Hypothesised median:", "middle")
        .expect("median");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be a number"), "{}", ui.status());
}

#[test]
fn the_chi_square_test_dialog_counts_the_two_way_table() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Chi-Square Test…"]);
    ui.select("Row:", "batch").expect("row");
    ui.select("Column:", "line").expect("column");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Chi-Square Test"), "{session}");
    assert!(session.contains("batch"), "{session}");
}

#[test]
fn the_chi_square_test_dialog_accepts_a_wider_table() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Chi-Square Test…"]);
    ui.select("Row:", "part").expect("row");
    ui.select("Column:", "line").expect("column");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Chi-Square Test"), "{}", ui.session());
}

#[test]
fn the_chi_square_test_dialog_rejects_the_same_variable_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Chi-Square Test…"]);
    ui.select("Row:", "batch").expect("row");
    ui.select("Column:", "batch").expect("column");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("different variables"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_two_way_anova_dialog_fits_both_factors() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Two-Way ANOVA…"]);
    ui.select("Factor A:", "batch").expect("A");
    ui.select("Factor B:", "line").expect("B");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Two-Way ANOVA"), "{session}");
    assert!(session.contains("batch"), "{session}");
    assert!(session.contains("line"), "{session}");
}

#[test]
fn the_two_way_anova_dialog_reports_the_interaction() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");

    // The default is the captured main-effects model.
    open(&mut ui, &["Stat", "Basic Statistics", "Two-Way ANOVA…"]);
    ui.select("Factor A:", "line").expect("A");
    ui.select("Factor B:", "part").expect("B");
    ui.click("Run").expect("run");
    assert!(
        !ui.session().contains("Interaction"),
        "the default must not fit the interaction: {}",
        ui.session()
    );

    // Ticking the box adds the interaction term.
    open(&mut ui, &["Stat", "Basic Statistics", "Two-Way ANOVA…"]);
    ui.select("Factor A:", "line").expect("A");
    ui.select("Factor B:", "part").expect("B");
    ui.click("Fit the interaction (A×B)").expect("toggle");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Interaction"), "{}", ui.session());
}

#[test]
fn the_two_way_anova_dialog_rejects_the_same_factor_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Two-Way ANOVA…"]);
    ui.select("Factor A:", "batch").expect("A");
    ui.select("Factor B:", "batch").expect("B");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("different factor columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_curve_fit_dialog_fits_the_linear_model() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Curve Fit…"]);
    ui.select("Y:", "height").expect("Y");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Curve Fit"), "{session}");
    assert!(session.contains("Polynomial"), "{session}");
}

#[test]
fn the_curve_fit_dialog_fits_the_exponential_model() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Curve Fit…"]);
    ui.select("Y:", "height").expect("Y");
    ui.select("Model:", "Exponential").expect("model");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Exponential"), "{session}");
    assert!(session.contains("R²"), "{session}");
}

#[test]
fn the_curve_fit_dialog_needs_both_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Curve Fit…"]);
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Select the Y column"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_principal_components_dialog_scores_the_components() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Principal Components…"]);
    ui.set_text("Columns:", "weight,height,pressure")
        .expect("columns");
    ui.set_text("Score components:", "2").expect("components");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Principal Component"), "{session}");
    assert!(session.contains("PC2"), "{session}");
}

#[test]
fn the_principal_components_dialog_can_keep_only_the_first() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Principal Components…"]);
    ui.set_text("Columns:", "weight,height").expect("columns");
    ui.set_text("Score components:", "1").expect("components");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Score PC1"), "{session}");
    assert!(!session.contains("Score PC2"), "{session}");
}

#[test]
fn the_principal_components_dialog_rejects_a_single_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Principal Components…"]);
    ui.set_text("Columns:", "weight").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_kmeans_dialog_standardises_the_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "K-Means Clustering…"]);
    ui.set_text("Columns:", "weight,height,pressure")
        .expect("columns");
    ui.set_text("Clusters:", "2").expect("clusters");
    ui.click("Standardise (z scores)").expect("standardise");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("K-Means"), "{session}");
    assert!(session.contains("Cluster"), "{session}");
}

#[test]
fn the_kmeans_dialog_accepts_a_custom_seed() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "K-Means Clustering…"]);
    ui.set_text("Columns:", "weight,height").expect("columns");
    ui.set_text("Seed:", "7").expect("seed");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("K-Means"), "{}", ui.session());
}

#[test]
fn the_kmeans_dialog_rejects_a_single_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "K-Means Clustering…"]);
    ui.set_text("Columns:", "weight").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_regression_dialog_fits_the_selected_model() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Regression…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Regression"), "{session}");
    assert!(session.contains("group[B]"), "{session}");
    assert!(session.contains("Predictor"), "{session}");
}

#[test]
fn the_regression_dialog_reads_the_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Regression…"]);
    ui.click("group").expect("drop the categorical predictor");
    ui.set_text("Confidence:", "99").expect("confidence");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("99.0% confidence"), "{session}");
    assert!(!session.contains("group"), "{session}");
}

#[test]
fn the_regression_dialog_needs_a_predictor() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Regression…"]);
    ui.click("x1").expect("drop x1");
    ui.click("x2").expect("drop x2");
    ui.click("group").expect("drop group");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least one predictor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_stepwise_dialog_runs_the_default_criterion() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Stepwise Regression…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Stepwise Regression"), "{session}");
    assert!(session.contains("Steps"), "{session}");
}

#[test]
fn the_stepwise_dialog_runs_the_aic_backward_search() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Stepwise Regression…"]);
    ui.click("Backward").expect("backward");
    ui.click("AIC").expect("AIC");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Stepwise Regression"), "{session}");
    assert!(session.contains("backward"), "{session}");
}

#[test]
fn the_stepwise_dialog_needs_a_candidate() {
    let mut ui = Automation::new();
    ui.load_frame(model_frame(), "model.csv");
    open(&mut ui, &["Stat", "Regression", "Stepwise Regression…"]);
    ui.click("x1").expect("drop x1");
    ui.click("x2").expect("drop x2");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least one candidate"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_logistic_dialog_fits_the_binary_response() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Binary Logistic Regression…"],
    );
    ui.select("Response (0/1):", "pass").expect("response");
    ui.set_text("Predictors:", "height").expect("predictors");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Logistic"), "{session}");
    assert!(session.contains("Odds"), "{session}");
}

#[test]
fn the_logistic_dialog_accepts_two_predictors() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Binary Logistic Regression…"],
    );
    ui.select("Response (0/1):", "pass").expect("response");
    ui.set_text("Predictors:", "height,pressure")
        .expect("predictors");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Logistic"), "{}", ui.session());
}

#[test]
fn the_logistic_dialog_needs_a_predictor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Binary Logistic Regression…"],
    );
    ui.select("Response (0/1):", "pass").expect("response");
    ui.set_text("Predictors:", "").expect("predictors");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least one predictor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_poisson_dialog_fits_the_count_response() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Regression", "Poisson Regression…"]);
    ui.select("Response (counts):", "defects")
        .expect("response");
    ui.set_text("Predictors:", "height").expect("predictors");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Poisson"), "{session}");
    assert!(session.contains("Deviance"), "{session}");
}

#[test]
fn the_poisson_dialog_accepts_two_predictors() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Regression", "Poisson Regression…"]);
    ui.select("Response (counts):", "defects")
        .expect("response");
    ui.set_text("Predictors:", "height,pressure")
        .expect("predictors");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Poisson"), "{}", ui.session());
}

#[test]
fn the_poisson_dialog_rejects_the_response_as_predictor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Regression", "Poisson Regression…"]);
    ui.select("Response (counts):", "defects")
        .expect("response");
    ui.set_text("Predictors:", "defects").expect("predictors");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("cannot also be a predictor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_ordinal_dialog_fits_the_coded_response() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Ordinal Logistic Regression…"],
    );
    ui.select("Response (codes):", "grade").expect("response");
    ui.set_text("Predictors:", "height").expect("predictors");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Ordinal"), "{session}");
    assert!(session.contains("Threshold"), "{session}");
}

#[test]
fn the_ordinal_dialog_accepts_two_predictors() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Ordinal Logistic Regression…"],
    );
    ui.select("Response (codes):", "grade").expect("response");
    ui.set_text("Predictors:", "height,pressure")
        .expect("predictors");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Ordinal"), "{}", ui.session());
}

#[test]
fn the_ordinal_dialog_rejects_the_response_as_predictor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Regression", "Ordinal Logistic Regression…"],
    );
    ui.select("Response (codes):", "grade").expect("response");
    ui.set_text("Predictors:", "grade").expect("predictors");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("cannot also be a predictor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_one_way_anova_dialog_reads_the_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "One-Way…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("One-Way ANOVA"), "{session}");
    assert!(session.contains("batch"), "{session}");
}

#[test]
fn the_one_way_anova_dialog_switches_to_levene() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "One-Way…"]);
    ui.select("Test:", "Levene Test").expect("test");
    ui.select("Factor:", "batch").expect("factor");
    ui.select("Centre:", "median").expect("centre");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Levene"), "{session}");
    assert!(session.contains("median"), "{session}");
}

#[test]
fn the_one_way_anova_dialog_rejects_the_response_as_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "One-Way…"]);
    ui.select("Factor:", "weight").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be different"), "{}", ui.status());
}

#[test]
fn the_levene_dialog_reads_the_centre() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Levene Test…"]);
    ui.select("Test:", "One-Way ANOVA").expect("test");
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("One-Way ANOVA"), "{}", ui.session());
}

#[test]
fn the_levene_dialog_accepts_the_median_centre() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Levene Test…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.select("Centre:", "median").expect("centre");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Levene"), "{session}");
    assert!(session.contains("median"), "{session}");
}

#[test]
fn the_levene_dialog_rejects_the_response_as_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Levene Test…"]);
    ui.select("Factor:", "weight").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be different"), "{}", ui.status());
}

#[test]
fn the_bartlett_dialog_runs_on_the_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Bartlett Test…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Bartlett"), "{session}");
}

#[test]
fn the_bartlett_dialog_accepts_three_levels() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Bartlett Test…"]);
    ui.select("Factor:", "part").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Bartlett"), "{}", ui.session());
}

#[test]
fn the_bartlett_dialog_rejects_the_response_as_factor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "ANOVA", "Bartlett Test…"]);
    ui.select("Factor:", "weight").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be different"), "{}", ui.status());
}

#[test]
fn the_calculator_dialog_adds_a_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Calc", "Calculator…"]);
    ui.set_text("New column:", "ratio").expect("name");
    ui.set_text("Expression:", "[weight] / [height]")
        .expect("expression");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("frame");
    assert!(data.column("ratio").is_ok(), "the column exists");
}

#[test]
fn the_calculator_dialog_reads_the_functions() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Calc", "Calculator…"]);
    ui.set_text("New column:", "root").expect("name");
    ui.set_text("Expression:", "sqrt([weight]) + abs([height])")
        .expect("expression");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("frame");
    assert!(data.column("root").is_ok(), "the column exists");
}

#[test]
fn the_calculator_dialog_rejects_a_broken_expression() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Calc", "Calculator…"]);
    ui.set_text("New column:", "broken").expect("name");
    ui.set_text("Expression:", "[weight] * (")
        .expect("expression");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    let data = ui.app().data().expect("frame");
    assert!(data.column("broken").is_err(), "no column was added");
}

#[test]
fn the_descriptive_statistics_by_dialog_groups_the_report() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &[
            "Stat",
            "Basic Statistics",
            "Display Descriptive Statistics by…",
        ],
    );
    ui.select("Numeric column:", "weight").expect("column");
    ui.select("By column:", "batch").expect("by");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Descriptive Statistics"), "{session}");
    assert!(session.contains("batch"), "{session}");
    assert!(session.contains('A') && session.contains('B'), "{session}");
}

#[test]
fn the_descriptive_statistics_by_dialog_accepts_six_groups() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &[
            "Stat",
            "Basic Statistics",
            "Display Descriptive Statistics by…",
        ],
    );
    ui.select("Numeric column:", "weight").expect("column");
    ui.select("By column:", "part").expect("by");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("P1"), "{session}");
    assert!(session.contains("P6"), "{session}");
}

#[test]
fn the_descriptive_statistics_by_dialog_needs_a_grouping_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &[
            "Stat",
            "Basic Statistics",
            "Display Descriptive Statistics by…",
        ],
    );
    ui.click("OK").expect("the disabled OK does nothing");
    assert!(ui.session().is_empty(), "{}", ui.session());
    assert_eq!(ui.window_titles(), vec!["Descriptive Statistics by Column"]);
}

#[test]
fn the_moods_median_dialog_compares_the_groups() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Mood's Median Test…"],
    );
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Mood"), "{session}");
    assert!(session.contains("batch"), "{session}");
}

#[test]
fn the_moods_median_dialog_accepts_three_levels() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Mood's Median Test…"],
    );
    ui.select("Factor:", "part").expect("factor");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Mood"), "{}", ui.session());
}

#[test]
fn the_moods_median_dialog_rejects_a_unique_level_per_row() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Basic Statistics", "Mood's Median Test…"],
    );
    ui.select("Factor:", "weight").expect("factor");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("Mood"), "{}", ui.session());
}

#[test]
fn the_one_proportion_dialog_uses_the_exact_method() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Proportion…"]);
    ui.select("First column:", "pass").expect("column");
    ui.set_text("Null proportion:", "0.5").expect("null");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("1 Proportion"), "{session}");
    assert!(
        session.contains("Clopper") || session.contains("Exact"),
        "{session}"
    );
}

#[test]
fn the_one_proportion_dialog_switches_to_the_normal_approximation() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Proportion…"]);
    ui.select("First column:", "pass").expect("column");
    ui.select("Method:", "normal").expect("method");
    ui.select("Alternative:", "greater").expect("alternative");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Wilson"), "{session}");
    assert!(session.contains("greater"), "{session}");
}

#[test]
fn the_one_proportion_dialog_rejects_a_null_proportion_above_one() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Proportion…"]);
    ui.select("First column:", "pass").expect("column");
    ui.set_text("Null proportion:", "1.5").expect("null");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("1 Proportion"), "{}", ui.session());
}

#[test]
fn the_two_proportions_dialog_compares_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Proportions…"]);
    ui.select("First column:", "pass").expect("first");
    ui.select("Second column:", "pass2").expect("second");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("2 Proportions"), "{session}");
    assert!(session.contains("pass2"), "{session}");
}

#[test]
fn the_two_proportions_dialog_reads_the_alternative() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Proportions…"]);
    ui.select("First column:", "pass").expect("first");
    ui.select("Second column:", "pass2").expect("second");
    ui.select("Alternative:", "less").expect("alternative");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("less alternative"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_two_proportions_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Proportions…"]);
    ui.select("First column:", "pass").expect("first");
    ui.select("Second column:", "pass").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_one_variance_dialog_tests_the_null_variance() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Variance…"]);
    ui.set_text("Null variance:", "0.5").expect("variance");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("1 Variance"), "{session}");
    assert!(session.contains("0.5"), "{session}");
}

#[test]
fn the_one_variance_dialog_reads_the_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Variance…"]);
    ui.set_text("Confidence:", "99").expect("confidence");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("99.0%"), "{}", ui.session());
}

#[test]
fn the_one_variance_dialog_rejects_a_negative_variance() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "1 Variance…"]);
    ui.set_text("Null variance:", "-1").expect("variance");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("1 Variance"), "{}", ui.session());
}

#[test]
fn the_two_variances_dialog_runs_the_f_test() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Variances…"]);
    ui.select("Second column:", "height").expect("second");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("2 Variances"), "{session}");
    assert!(session.contains("F test"), "{session}");
}

#[test]
fn the_two_variances_dialog_switches_to_bonett() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Variances…"]);
    ui.select("Second column:", "height").expect("second");
    ui.select("Method:", "Bonett test").expect("method");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Bonett"), "{}", ui.session());
}

#[test]
fn the_two_variances_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "2 Variances…"]);
    ui.select("Second column:", "weight").expect("second");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("different columns"), "{}", ui.status());
}

#[test]
fn the_probability_plot_dialog_draws_the_selected_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.app_mut().select_cell(0, 0);
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Probability Plot"), "{session}");
    assert!(session.contains("Normal"), "{session}");
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_probability_plot_dialog_reads_the_distribution_and_display() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.app_mut().select_cell(0, 0);
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.select("Distribution:", "Lognormal")
        .expect("distribution");
    ui.select("Display:", "Fitted line only").expect("display");
    // The band starts on, so two clicks leave it on again and prove the toggle.
    ui.click("Confidence band").expect("band off");
    ui.click("Confidence band").expect("band on");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Lognormal"), "{session}");
    assert!(session.contains("Fitted line only"), "{session}");
    let plot = ui.plot().expect("plot");
    assert!(!plot.bands.is_empty(), "the confidence band is drawn");
}

#[test]
fn the_probability_plot_dialog_needs_a_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.click("weight").expect("deselect the only column");
    ui.click("OK").expect("ok");
    assert!(ui.session().is_empty(), "{}", ui.session());
    assert!(
        !ui.status().is_empty(),
        "the status bar asks for a column: {}",
        ui.status()
    );
}

#[test]
fn the_scatterplot_dialog_reads_both_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Scatterplot…"]);
    ui.select("X:", "weight").expect("X");
    ui.select("Y:", "height").expect("Y");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Scatterplot"), "{session}");
    let plot = ui.plot().expect("plot");
    assert!(
        !plot.lines.is_empty(),
        "the least-squares line is on by default"
    );
}

#[test]
fn the_scatterplot_dialog_draws_the_least_squares_line() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Scatterplot…"]);
    ui.select("Y:", "pressure").expect("Y");
    ui.click("Least-squares line").expect("fit");
    ui.click("Run").expect("run");
    let plot = ui.plot().expect("plot");
    assert!(plot.lines.is_empty(), "the fit is switched off");
}

#[test]
fn the_scatterplot_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Scatterplot…"]);
    ui.select("Y:", "weight").expect("Y");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("must be different"), "{}", ui.status());
}

/// A worksheet with text columns only, so a dialog that needs a numeric
/// column opens with an empty selection.
fn text_only() -> DataFrame {
    DataFrame::new(vec![Column::new_text(
        "label",
        vec!["x".to_owned(), "y".to_owned(), "z".to_owned()],
    )])
    .expect("text only")
}

/// An L9 orthogonal array with three factors at levels 1..3.
fn taguchi_frame() -> DataFrame {
    let runs = [
        (1, 1, 1),
        (1, 2, 2),
        (1, 3, 3),
        (2, 1, 2),
        (2, 2, 3),
        (2, 3, 1),
        (3, 1, 3),
        (3, 2, 1),
        (3, 3, 2),
    ];
    let y: Vec<f64> = runs
        .iter()
        .enumerate()
        .map(|(index, (a, b, c))| {
            10.0 + 2.0 * f64::from(*a == 1)
                + 1.5 * f64::from(*b == 1)
                + 0.5 * f64::from(*c == 1)
                + 0.05 * index as f64
        })
        .collect();
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_f64("A", runs.iter().map(|run| run.0 as f64).collect()),
        Column::new_f64("B", runs.iter().map(|run| run.1 as f64).collect()),
        Column::new_f64("C", runs.iter().map(|run| run.2 as f64).collect()),
    ])
    .expect("taguchi frame")
}

/// A replicated 2³ factorial with a block column.
fn factorial_frame() -> DataFrame {
    let mut y = Vec::new();
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    let mut block = Vec::new();
    let mut index = 0;
    for replicate in 0..2 {
        for first in [-1.0, 1.0] {
            for second in [-1.0, 1.0] {
                for third in [-1.0, 1.0] {
                    y.push(
                        5.0 + 2.0 * first + second - 1.5 * third
                            + 0.5 * first * second
                            + 0.03 * index as f64,
                    );
                    a.push(first);
                    b.push(second);
                    c.push(third);
                    block.push(if replicate == 0 { "B1" } else { "B2" }.to_owned());
                    index += 1;
                }
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("C", c),
        Column::new_text("block", block),
    ])
    .expect("factorial frame")
}

/// A 3² grid with no linear term, so the surface is flat at the centre.
fn flat_rsm_frame() -> DataFrame {
    let mut y = Vec::new();
    let mut a = Vec::new();
    let mut b = Vec::new();
    for first in [-1.0, 0.0, 1.0] {
        for second in [-1.0, 0.0, 1.0] {
            y.push(5.0 - first * first - second * second);
            a.push(first);
            b.push(second);
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_f64("A", a),
        Column::new_f64("B", b),
    ])
    .expect("flat rsm frame")
}

/// A 3² grid with a quadratic response, enough for a response surface.
fn rsm_frame() -> DataFrame {
    let mut y = Vec::new();
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut index = 0;
    for first in [-1.0, 0.0, 1.0] {
        for second in [-1.0, 0.0, 1.0] {
            y.push(
                10.0 + 2.0 * first - second + 0.5 * first * second - 1.2 * first * first
                    + 0.7 * second * second
                    + 0.01 * index as f64,
            );
            a.push(first);
            b.push(second);
            index += 1;
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_f64("A", a),
        Column::new_f64("B", b),
    ])
    .expect("rsm frame")
}

/// Three independent numeric columns for the Hotelling T² chart.
fn hotelling_frame() -> DataFrame {
    let n = 30;
    let first: Vec<f64> = (0..n).map(|i| ((i * 37 % 17) as f64) / 3.0).collect();
    let second: Vec<f64> = (0..n).map(|i| ((i * 53 % 19) as f64) / 5.0).collect();
    let third: Vec<f64> = (0..n).map(|i| ((i * 29 % 23) as f64) / 7.0).collect();
    DataFrame::new(vec![
        Column::new_f64("x1", first),
        Column::new_f64("x2", second),
        Column::new_f64("x3", third),
    ])
    .expect("hotelling frame")
}

/// A crossed Gage R&R study: six parts, two operators, two replicates.
fn gage_frame() -> DataFrame {
    let mut measurement = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    let mut index = 0;
    for number in 1..=6 {
        for name in ["A", "B"] {
            for _ in 0..2 {
                measurement.push(
                    10.0 + number as f64 * 0.5
                        + if name == "B" { 0.2 } else { 0.0 }
                        + 0.02 * index as f64,
                );
                part.push(format!("P{number}"));
                operator.push(name.to_owned());
                index += 1;
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measurement", measurement),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("gage frame")
}

#[test]
fn the_taguchi_design_dialog_generates_l8() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Design…"]);
    ui.click("Generate").expect("generate");
    assert!(
        ui.session().contains("Taguchi design generated: L8"),
        "{}",
        ui.session()
    );
    let data = ui.app().data().expect("frame");
    assert_eq!(data.n_rows(), 8);
    assert!(data.column("A").is_ok() && data.column("C").is_ok());
}

#[test]
fn the_taguchi_design_dialog_generates_l27() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Design…"]);
    ui.click("L27").expect("L27");
    ui.set_text("Factors:", "13").expect("factors");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session().contains("Taguchi design generated: L27"),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 27);
}

#[test]
fn the_taguchi_design_dialog_rejects_too_many_factors() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Design…"]);
    ui.click("L8").expect("L8");
    ui.set_text("Factors:", "15").expect("factors");
    ui.click("Generate").expect("generate");
    assert!(!ui.status().contains("generated"), "{}", ui.status());
    assert_eq!(ui.app().data().expect("frame").n_rows(), 24);
}

#[test]
fn the_taguchi_analysis_dialog_ranks_the_factors() {
    let mut ui = Automation::new();
    ui.load_frame(taguchi_frame(), "taguchi.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Analysis…"]);
    ui.select("Response:", "y").expect("response");
    ui.set_text("Factors:", "A,B,C").expect("factors");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Taguchi Analysis — response y"),
        "{session}"
    );
    assert!(session.contains("Signal-to-noise ratios"), "{session}");
    assert!(session.contains("Strongest factor"), "{session}");
}

#[test]
fn the_taguchi_analysis_dialog_reads_the_quality_characteristic() {
    let mut ui = Automation::new();
    ui.load_frame(taguchi_frame(), "taguchi.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Analysis…"]);
    ui.select("Response:", "y").expect("response");
    ui.set_text("Factors:", "A,B").expect("factors");
    ui.click("Larger is better").expect("quality");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("larger is better"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_taguchi_analysis_dialog_rejects_the_response_as_factor() {
    let mut ui = Automation::new();
    ui.load_frame(taguchi_frame(), "taguchi.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Analysis…"]);
    ui.select("Response:", "y").expect("response");
    ui.set_text("Factors:", "y").expect("factors");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("response cannot also be a factor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_sampling_dialog_evaluates_a_plan() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Acceptance Sampling…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Acceptance Sampling — OC Curve"),
        "{session}"
    );
    assert!(session.contains("Single sampling"), "{session}");
}

#[test]
fn the_sampling_dialog_plans_a_scheme() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Acceptance Sampling…"]);
    ui.click("Plan a scheme").expect("plan");
    ui.set_text("AQL %:", "1").expect("aql");
    ui.set_text("LTPD %:", "5").expect("ltpd");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Acceptance Sampling — Plan"), "{session}");
    assert!(session.contains("AQL 1.00%"), "{session}");
}

#[test]
fn the_sampling_dialog_rejects_an_acceptance_number_above_the_sample() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Acceptance Sampling…"]);
    ui.set_text("Sample size:", "10").expect("sample");
    ui.set_text("Acceptance:", "10").expect("acceptance");
    ui.click("Run").expect("run");
    assert!(
        ui.status()
            .contains("acceptance number must be below the sample size"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_series_plot_dialog_plots_the_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Time Series Plot…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Time Series Plot — column weight"),
        "{session}"
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_series_plot_dialog_reads_the_time_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Time Series Plot…"]);
    ui.select("Time (optional):", "height").expect("time");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Observations in the order of height"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_series_plot_dialog_rejects_the_series_as_time() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Time Series Plot…"]);
    ui.select("Time (optional):", "weight").expect("time");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("time column must be different"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_stem_dialog_displays_the_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Stem-and-Leaf…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Stem-and-Leaf Display — column weight"),
        "{session}"
    );
    assert!(session.contains("Leaf unit"), "{session}");
}

#[test]
fn the_stem_dialog_accepts_a_coded_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Stem-and-Leaf…"]);
    ui.select("Column:", "grade").expect("column");
    ui.click("Run").expect("run");
    assert!(
        ui.session()
            .contains("Stem-and-Leaf Display — column grade"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_stem_dialog_needs_a_numeric_column() {
    let mut ui = Automation::new();
    ui.load_frame(text_only(), "labels.csv");
    open(&mut ui, &["Stat", "Multivariate", "Stem-and-Leaf…"]);
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("cannot be used as a numeric sample"),
        "{}",
        ui.status()
    );
    assert!(!ui.session().contains("Stem-and-Leaf Display"));
}

#[test]
fn the_ecdf_dialog_draws_the_step_function() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Empirical CDF…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Empirical CDF — column weight"),
        "{session}"
    );
    assert!(session.contains("Dvoretzky"), "{session}");
}

#[test]
fn the_ecdf_dialog_reads_the_band_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Empirical CDF…"]);
    ui.set_text("Band confidence:", "50").expect("confidence");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("50% Dvoretzky"), "{}", ui.session());
}

#[test]
fn the_ecdf_dialog_needs_a_numeric_column() {
    let mut ui = Automation::new();
    ui.load_frame(text_only(), "labels.csv");
    open(&mut ui, &["Stat", "Multivariate", "Empirical CDF…"]);
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("cannot be used as a numeric sample"),
        "{}",
        ui.status()
    );
    assert!(!ui.session().contains("Empirical CDF"));
}

#[test]
fn the_dotplot_dialog_draws_the_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Dotplot…"]);
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Dotplot — weight"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_dotplot_dialog_reads_the_grouping_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Dotplot…"]);
    ui.select("By (optional):", "batch").expect("by");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Dotplot — weight by batch"), "{session}");
    assert!(session.contains("Groups"), "{session}");
}

#[test]
fn the_dotplot_dialog_rejects_the_column_as_group() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Dotplot…"]);
    ui.select("By (optional):", "weight").expect("by");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("factor must be a different column"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_matrix_plot_dialog_draws_every_pair() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Matrix Plot…"]);
    ui.set_text("Columns:", "weight,height,pressure")
        .expect("columns");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Matrix Plot"), "{}", ui.session());
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_matrix_plot_dialog_accepts_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Matrix Plot…"]);
    ui.set_text("Columns:", "weight,height").expect("columns");
    ui.set_text("Bins on the diagonal:", "4").expect("bins");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Matrix Plot"), "{}", ui.session());
}

#[test]
fn the_matrix_plot_dialog_needs_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Matrix Plot…"]);
    ui.set_text("Columns:", "weight").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_hotelling_dialog_draws_the_chart() {
    let mut ui = Automation::new();
    ui.load_frame(hotelling_frame(), "hotelling.csv");
    open(&mut ui, &["Stat", "Multivariate", "Hotelling T² Chart…"]);
    ui.set_text("Columns:", "x1,x2,x3").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Hotelling T² Chart"),
        "{}",
        ui.session()
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_hotelling_dialog_reads_the_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(hotelling_frame(), "hotelling.csv");
    open(&mut ui, &["Stat", "Multivariate", "Hotelling T² Chart…"]);
    ui.set_text("Columns:", "x1,x2").expect("columns");
    ui.set_text("Subgroup:", "4").expect("subgroup");
    ui.set_text("Confidence:", "90").expect("confidence");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Phase I limits at 90.0%"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_hotelling_dialog_needs_two_columns() {
    let mut ui = Automation::new();
    ui.load_frame(hotelling_frame(), "hotelling.csv");
    open(&mut ui, &["Stat", "Multivariate", "Hotelling T² Chart…"]);
    ui.set_text("Columns:", "weight").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_moving_average_dialog_centres_the_window() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Moving Average…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Centred moving average of 3 observations"),
        "{session}"
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_moving_average_dialog_uses_the_two_by_w_rule() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Moving Average…"]);
    ui.set_text("Window:", "2").expect("window");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("2 observations (2×w rule)"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_moving_average_dialog_rejects_a_window_longer_than_the_series() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Moving Average…"]);
    ui.set_text("Window:", "100").expect("window");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("window is longer than the series"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_decomposition_dialog_splits_the_series() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Decomposition…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Classical additive decomposition with a seasonal period of 12"),
        "{session}"
    );
    assert!(session.contains("Seasonal indices"), "{session}");
}

#[test]
fn the_decomposition_dialog_reads_the_multiplicative_model() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Decomposition…"]);
    ui.click("Multiplicative").expect("model");
    ui.set_text("Period:", "4").expect("period");
    ui.click("Run").expect("run");
    assert!(
        ui.session()
            .contains("Classical multiplicative decomposition with a seasonal period of 4"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_decomposition_dialog_rejects_a_period_over_half_the_series() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Decomposition…"]);
    ui.set_text("Period:", "13").expect("period");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("sample too small"), "{}", ui.status());
}

#[test]
fn the_smoothing_dialog_forecasts_the_series() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Exponential Smoothing…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Holt-Winters additive smoothing"),
        "{session}"
    );
    assert!(session.contains("Forecast"), "{session}");
}

#[test]
fn the_smoothing_dialog_reads_the_season_and_horizon() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Time Series", "Exponential Smoothing…"]);
    ui.set_text("Season:", "2").expect("season");
    ui.set_text("Forecast periods:", "1").expect("horizon");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Seasonal period"), "{session}");
    assert!(session.contains("2.000000"), "{session}");
}

#[test]
fn the_smoothing_dialog_needs_enough_observations() {
    let mut ui = Automation::new();
    ui.load_frame(tiny(), "tiny.csv");
    open(&mut ui, &["Stat", "Time Series", "Exponential Smoothing…"]);
    ui.click("Run").expect("run");
    assert!(ui.status().contains("sample too small"), "{}", ui.status());
}

#[test]
fn the_create_design_dialog_generates_a_factorial() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Create 2-Level Design…"]);
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("DOE design generated: 8 runs, factors A, B."),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 8);
}

#[test]
fn the_create_design_dialog_reads_the_factors_and_replicates() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Create 2-Level Design…"]);
    ui.set_text("Factors:", "A,B,C").expect("factors");
    ui.set_text("Replicates:", "1").expect("replicates");
    ui.set_text("Center points:", "1").expect("centers");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("DOE design generated: 9 runs, factors A, B, C."),
        "{}",
        ui.session()
    );
}

#[test]
fn the_create_design_dialog_needs_a_factor_name() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Create 2-Level Design…"]);
    ui.set_text("Factors:", "").expect("factors");
    ui.click("Generate").expect("generate");
    assert!(
        ui.status().contains("Enter at least one factor name"),
        "{}",
        ui.status()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 24);
}

#[test]
fn the_plackett_burman_dialog_generates_twelve_runs() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Plackett-Burman Design…"]);
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Plackett-Burman design generated: 12 runs, 11 factors."),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 12);
}

#[test]
fn the_plackett_burman_dialog_reads_the_run_count() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Plackett-Burman Design…"]);
    ui.set_text("Runs:", "20").expect("runs");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Plackett-Burman design generated: 20 runs, 19 factors."),
        "{}",
        ui.session()
    );
}

#[test]
fn the_plackett_burman_dialog_rejects_an_unsupported_run_count() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Plackett-Burman Design…"]);
    ui.set_text("Runs:", "10").expect("runs");
    ui.click("Generate").expect("generate");
    assert!(!ui.status().contains("generated"), "{}", ui.status());
    assert_eq!(ui.app().data().expect("frame").n_rows(), 24);
}

#[test]
fn the_box_behnken_dialog_generates_fifteen_runs() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Box-Behnken Design…"]);
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Box-Behnken design generated: 15 runs, factors A, B, C."),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 15);
}

#[test]
fn the_box_behnken_dialog_reads_the_center_points() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Box-Behnken Design…"]);
    ui.set_text("Center points:", "1").expect("centers");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Box-Behnken design generated: 13 runs, factors A, B, C."),
        "{}",
        ui.session()
    );
}

#[test]
fn the_box_behnken_dialog_needs_three_factor_names() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Box-Behnken Design…"]);
    ui.set_text("Factors:", "").expect("factors");
    ui.click("Generate").expect("generate");
    assert!(
        ui.status()
            .contains("A Box-Behnken design needs at least three factors"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_factorial_dialog_fits_the_main_effects() {
    let mut ui = Automation::new();
    ui.load_frame(factorial_frame(), "factorial.csv");
    open(&mut ui, &["Stat", "DOE", "2-Level Factorial…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("2-Level Factorial — response y"),
        "{session}"
    );
    assert!(session.contains("A"), "{session}");
}

#[test]
fn the_factorial_dialog_reads_the_block() {
    let mut ui = Automation::new();
    ui.load_frame(factorial_frame(), "factorial.csv");
    open(&mut ui, &["Stat", "DOE", "2-Level Factorial…"]);
    ui.click("C").expect("drop C");
    ui.select("Block:", "block").expect("block");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("2-Level Factorial — response y"),
        "{session}"
    );
    assert!(session.contains("block"), "{session}");
}

#[test]
fn the_factorial_dialog_needs_a_factor() {
    let mut ui = Automation::new();
    ui.load_frame(factorial_frame(), "factorial.csv");
    open(&mut ui, &["Stat", "DOE", "2-Level Factorial…"]);
    ui.click("A").expect("drop A");
    ui.click("B").expect("drop B");
    ui.click("C").expect("drop C");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Select at least one factor"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_response_surface_dialog_fits_the_quadratic() {
    let mut ui = Automation::new();
    ui.load_frame(rsm_frame(), "rsm.csv");
    open(&mut ui, &["Stat", "DOE", "Response Surface…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Response Surface — response y"),
        "{session}"
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_response_surface_dialog_reads_the_stationary_point() {
    let mut ui = Automation::new();
    ui.load_frame(rsm_frame(), "rsm.csv");
    open(&mut ui, &["Stat", "DOE", "Response Surface…"]);
    ui.click("Stationary point and eigenvalues")
        .expect("stationary");
    ui.click("Contour plot (two factors)").expect("contours");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Stationary point"), "{session}");
    assert!(session.contains("Eigenvalue"), "{session}");
}

#[test]
fn the_response_surface_dialog_needs_two_factors() {
    let mut ui = Automation::new();
    ui.load_frame(rsm_frame(), "rsm.csv");
    open(&mut ui, &["Stat", "DOE", "Response Surface…"]);
    ui.click("B").expect("drop B");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Select at least two factors"),
        "{}",
        ui.status()
    );
}

fn unstable_frame() -> DataFrame {
    let mut values = vec![5.0; 24];
    values[10] = 50.0;
    DataFrame::new(vec![Column::new_f64("value", values)]).expect("unstable frame")
}

fn constant_frame() -> DataFrame {
    DataFrame::new(vec![Column::new_f64("value", vec![5.0; 24])]).expect("constant frame")
}

fn empty_frame() -> DataFrame {
    DataFrame::new(vec![Column::new_f64("value", Vec::new())]).expect("empty frame")
}

#[test]
fn the_alt_dialog_fits_the_arrhenius_weibull_model() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Accelerated Life Testing — Arrhenius (1/S) / Weibull"),
        "{session}"
    );
    assert!(session.contains("Use stress"), "{session}");
}

#[test]
fn the_alt_dialog_reads_the_life_and_acceleration_models() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.click("Lognormal").expect("life");
    ui.click("Power").expect("acceleration");
    ui.set_text("Use stress:", "250").expect("use stress");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Lognormal"), "{session}");
    assert!(session.contains("inverse power"), "{session}");
    assert!(session.contains("250"), "{session}");
}

#[test]
fn the_alt_dialog_rejects_the_time_as_stress() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.select("Stress:", "weight").expect("stress");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("must be different columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_kaplan_meier_dialog_estimates_the_survival() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Kaplan–Meier…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Kaplan–Meier — column weight"),
        "{session}"
    );
    assert!(session.contains("Median survival"), "{session}");
}

#[test]
fn the_kaplan_meier_dialog_reads_the_censoring_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Kaplan–Meier…"]);
    ui.select("Censoring column:", "pass").expect("censor");
    ui.set_text("Censoring value:", "1").expect("value");
    ui.set_text("Confidence:", "50").expect("confidence");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Censored"), "{session}");
    assert!(session.contains("50.0%"), "{session}");
}

#[test]
fn the_kaplan_meier_dialog_rejects_a_non_numeric_censoring_value() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Kaplan–Meier…"]);
    ui.set_text("Censoring value:", "x").expect("value");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Censoring value must be a number"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_weibull_dialog_fits_the_censored_life() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Weibull (censored)…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Weibull Reliability — column weight"),
        "{session}"
    );
    assert!(session.contains("Shape (α)"), "{session}");
}

#[test]
fn the_weibull_dialog_reads_the_censoring_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Weibull (censored)…"]);
    ui.select("Censoring column:", "pass").expect("censor");
    ui.set_text("Censoring value:", "1").expect("value");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Censored"), "{}", ui.session());
}

#[test]
fn the_weibull_dialog_rejects_a_non_numeric_censoring_value() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Weibull (censored)…"]);
    ui.set_text("Censoring value:", "x").expect("value");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Censoring value must be a number"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_exponential_dialog_fits_the_censored_life() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Exponential (censored)…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Exponential Reliability — column weight"),
        "{session}"
    );
    assert!(session.contains("Mean/scale"), "{session}");
}

#[test]
fn the_exponential_dialog_reads_the_censoring_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Exponential (censored)…"]);
    ui.select("Censoring column:", "pass").expect("censor");
    ui.set_text("Censoring value:", "1").expect("value");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Censored"), "{}", ui.session());
}

#[test]
fn the_exponential_dialog_rejects_a_non_numeric_censoring_value() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Exponential (censored)…"]);
    ui.set_text("Censoring value:", "x").expect("value");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Censoring value must be a number"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_lognormal_dialog_fits_the_censored_life() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Lognormal (censored)…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Lognormal Reliability — column weight"),
        "{session}"
    );
    assert!(session.contains("Location (µ)"), "{session}");
}

#[test]
fn the_lognormal_dialog_reads_the_censoring_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Lognormal (censored)…"]);
    ui.select("Censoring column:", "pass").expect("censor");
    ui.set_text("Censoring value:", "1").expect("value");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Censored"), "{}", ui.session());
}

#[test]
fn the_lognormal_dialog_rejects_a_non_numeric_censoring_value() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Reliability", "Lognormal (censored)…"]);
    ui.set_text("Censoring value:", "x").expect("value");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Censoring value must be a number"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_imr_chart_dialog_draws_the_individuals() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(
        session.contains("Control Chart: I–MR — column weight"),
        "{session}"
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_imr_chart_dialog_enables_a_special_cause_test() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    ui.select("Column:", "height").expect("column");
    ui.click("2").expect("test 2");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("same side of the center line"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_imr_chart_dialog_rejects_a_constant_process() {
    let mut ui = Automation::new();
    ui.load_frame(constant_frame(), "constant.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    ui.click("OK").expect("ok");
    assert!(ui.status().contains("constant"), "{}", ui.status());
}

#[test]
fn the_xbar_r_chart_dialog_reads_the_subgroup() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–R…"]);
    ui.set_text("Subgroup size:", "4").expect("subgroup");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("Control Chart: X̄–R — column weight"),
        "{}",
        ui.session()
    );
    assert!(ui.session().contains("4"), "{}", ui.session());
}

#[test]
fn the_xbar_r_chart_dialog_accepts_the_smallest_subgroup() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–R…"]);
    ui.set_text("Subgroup size:", "2").expect("subgroup");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("Control Chart: X̄–R — column weight"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_xbar_r_chart_dialog_rejects_a_subgroup_larger_than_the_sample() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–R…"]);
    ui.set_text("Subgroup size:", "25").expect("subgroup");
    ui.click("OK").expect("ok");
    assert!(ui.status().contains("sample too small"), "{}", ui.status());
}

#[test]
fn the_xbar_s_chart_dialog_draws_the_standard_deviation() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–S…"]);
    ui.set_text("Subgroup size:", "6").expect("subgroup");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(
        session.contains("Control Chart: X̄–S — column weight"),
        "{session}"
    );
    assert!(session.contains("standard deviation"), "{session}");
}

#[test]
fn the_xbar_s_chart_dialog_accepts_three_observations_per_group() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–S…"]);
    ui.set_text("Subgroup size:", "3").expect("subgroup");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("Control Chart: X̄–S — column weight"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_xbar_s_chart_dialog_rejects_a_subgroup_larger_than_the_sample() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–S…"]);
    ui.set_text("Subgroup size:", "25").expect("subgroup");
    ui.click("OK").expect("ok");
    assert!(ui.status().contains("sample too small"), "{}", ui.status());
}

#[test]
fn the_ewma_chart_dialog_smooths_the_series() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "EWMA…"]);
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(
        session.contains("Control Chart: EWMA — column weight"),
        "{session}"
    );
    assert!(session.contains("λ = 0.2"), "{session}");
}

#[test]
fn the_ewma_chart_dialog_reads_lambda_and_l() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "EWMA…"]);
    ui.set_text("λ:", "0.4").expect("lambda");
    ui.set_text("L:", "2.5").expect("limit");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("λ = 0.4"), "{session}");
    assert!(session.contains("L = 2.5σ"), "{session}");
}

#[test]
fn the_ewma_chart_dialog_rejects_a_constant_process() {
    let mut ui = Automation::new();
    ui.load_frame(constant_frame(), "constant.csv");
    open(&mut ui, &["Stat", "Control Charts", "EWMA…"]);
    ui.click("OK").expect("ok");
    assert!(ui.status().contains("constant"), "{}", ui.status());
}

#[test]
fn the_attribute_chart_dialog_draws_the_p_chart() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "P/NP/C/U Chart…"]);
    ui.select("Defects column:", "defects").expect("defects");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("P Chart — column defects"), "{session}");
    assert!(session.contains("100"), "{session}");
}

#[test]
fn the_attribute_chart_dialog_switches_to_the_c_chart() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "P/NP/C/U Chart…"]);
    ui.select("Chart:", "C").expect("chart");
    ui.select("Defects column:", "defects").expect("defects");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("C Chart — column defects"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_attribute_chart_dialog_rejects_non_integer_counts() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "P/NP/C/U Chart…"]);
    ui.select("Defects column:", "weight").expect("defects");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("counts must be non-negative integers"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_g_chart_dialog_draws_the_rare_events() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Control Charts", "G Chart (Rare Events)…"],
    );
    ui.select("Events:", "defects").expect("events");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("G Chart (Rare Events) — defects"),
        "{session}"
    );
}

#[test]
fn the_g_chart_dialog_reads_the_time_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Control Charts", "G Chart (Rare Events)…"],
    );
    ui.select("Events:", "grade").expect("events");
    ui.select("Times:", "weight").expect("times");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("G Chart (Rare Events) — grade"),
        "{session}"
    );
    assert!(session.contains("Time"), "{session}");
    assert!(session.contains("Events"), "{session}");
}

#[test]
fn the_g_chart_dialog_rejects_non_integer_events() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Control Charts", "G Chart (Rare Events)…"],
    );
    ui.select("Events:", "weight").expect("events");
    ui.click("Run").expect("run");
    assert!(
        ui.status()
            .contains("number of events must be a non-negative integer"),
        "{}",
        ui.status()
    );
}

/// A positive, right-skewed sample that every capability distribution fits.
fn capability_frame() -> DataFrame {
    let values = [
        1.2, 1.5, 1.8, 2.0, 2.1, 2.3, 2.5, 2.7, 2.9, 3.1, 3.4, 3.6, 3.9, 4.2, 4.6, 5.0, 5.5, 6.0,
        6.8, 7.5, 8.5, 10.0, 12.0, 15.0,
    ];
    DataFrame::new(vec![Column::new_f64("value", values.to_vec())]).expect("capability frame")
}

/// A nested Gage R&R study: two operators, three of their own parts each.
fn nested_gage_frame() -> DataFrame {
    let mut measurement = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    let mut index = 0;
    for (name, first_part) in [("A", 1), ("B", 4)] {
        for number in first_part..first_part + 3 {
            for _ in 0..2 {
                measurement.push(
                    10.0 + f64::from(number) * 0.5
                        + if name == "B" { 0.2 } else { 0.0 }
                        + 0.02 * index as f64,
                );
                part.push(format!("P{number}"));
                operator.push(name.to_owned());
                index += 1;
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measurement", measurement),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("nested gage frame")
}

/// A linearity study: eight reference parts with three repeats each.
fn linearity_frame() -> DataFrame {
    let mut measurement = Vec::new();
    let mut reference = Vec::new();
    let mut index = 0;
    for part in 1..=8 {
        let target = f64::from(part) * 10.0;
        for _ in 0..3 {
            measurement.push(target + 0.5 + 0.02 * (target - 45.0) + 0.01 * index as f64);
            reference.push(target);
            index += 1;
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measurement", measurement),
        Column::new_f64("reference", reference),
    ])
    .expect("linearity frame")
}

#[test]
fn the_capability_dialog_reports_the_normal_indices() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.set_text("LSL:", "20").expect("lsl");
    ui.set_text("USL:", "30").expect("usl");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(
        session.contains("Capability Analysis: Normal — column weight"),
        "{session}"
    );
    assert!(session.contains("Cp"), "{session}");
}

#[test]
fn the_capability_dialog_reads_the_distribution() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.select("Distribution:", "Lognormal")
        .expect("distribution");
    ui.set_text("LSL:", "20").expect("lsl");
    ui.click("OK").expect("ok");
    assert!(
        ui.session()
            .contains("Capability Analysis: Lognormal — column weight"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_capability_dialog_rejects_a_non_numeric_limit() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.set_text("LSL:", "abc").expect("lsl");
    ui.click("OK").expect("ok");
    assert!(
        ui.status().contains("invalid limit: abc"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

#[test]
fn the_distribution_comparison_compares_the_selected_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    assert!(
        ui.session()
            .contains("Distribution Comparison — column weight"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_distribution_comparison_needs_observations() {
    let mut ui = Automation::new();
    ui.load_frame(tiny(), "tiny.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    assert!(!ui.status().is_empty(), "the status bar explains the error");
}

#[test]
fn the_distribution_comparison_rejects_a_constant_column() {
    let mut ui = Automation::new();
    ui.load_frame(constant_frame(), "constant.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    assert!(
        !ui.status().contains("Distribution comparison:"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

#[test]
fn the_interval_plot_dialog_draws_the_groups() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Interval Plot…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Interval Plot — response weight by batch"),
        "{session}"
    );
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_interval_plot_dialog_reads_the_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Interval Plot…"]);
    ui.select("Factor:", "batch").expect("factor");
    ui.set_text("Confidence:", "50").expect("confidence");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("50.0% Student-t intervals"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_interval_plot_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Interval Plot…"]);
    ui.select("Factor:", "weight").expect("factor");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("must be different columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_pareto_dialog_counts_the_categories() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Pareto Chart…"]);
    ui.select("Column:", "batch").expect("column");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Pareto Chart — column batch"), "{session}");
    assert!(session.contains("Cumulative"), "{session}");
}

#[test]
fn the_pareto_dialog_accepts_a_numeric_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Pareto Chart…"]);
    ui.select("Column:", "defects").expect("column");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Pareto Chart — column defects"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_pareto_dialog_weights_the_rows() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Pareto Chart…"]);
    ui.select("Column:", "batch").expect("column");
    ui.select("Weight column (optional):", "weight")
        .expect("weight");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Pareto Chart — column batch, weighted by weight"),
        "{session}"
    );
    assert!(session.contains("Weight"), "{session}");
    assert!(
        session.contains("each row adds the value of weight"),
        "{session}"
    );
}

#[test]
fn the_pareto_dialog_rejects_a_text_weight() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Pareto Chart…"]);
    ui.select("Column:", "batch").expect("column");
    ui.select("Weight column (optional):", "line")
        .expect("weight");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("weight column must be numeric"),
        "{}",
        ui.status()
    );
    assert!(!ui.session().contains("Pareto Chart"));
}

#[test]
fn the_pareto_dialog_needs_observations() {
    let mut ui = Automation::new();
    ui.load_frame(empty_frame(), "empty.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Pareto Chart…"]);
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status bar explains the error");
    assert!(!ui.session().contains("Pareto Chart"));
}

#[test]
fn the_linearity_dialog_fits_the_reference_line() {
    let mut ui = Automation::new();
    ui.load_frame(linearity_frame(), "linearity.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Linearity…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Gage Linearity and Bias Study — measurement against reference"),
        "{session}"
    );
    assert!(session.contains("Reference parts"), "{session}");
}

#[test]
fn the_linearity_dialog_accepts_the_minimum_three_parts() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Linearity…"]);
    ui.select("Measurement:", "weight").expect("measurement");
    ui.select("Reference:", "grade").expect("reference");
    ui.click("Run").expect("run");
    assert!(
        ui.session()
            .contains("Gage Linearity and Bias Study — weight against grade"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_linearity_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(linearity_frame(), "linearity.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Linearity…"]);
    ui.select("Reference:", "measurement").expect("reference");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("must be different columns"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_agreement_dialog_measures_the_raters() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Attribute Agreement…"]);
    ui.select("Part:", "part").expect("part");
    ui.select("Appraiser:", "operator").expect("appraiser");
    ui.select("Rating:", "grade").expect("rating");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Attribute Agreement Analysis — part × operator × grade"),
        "{session}"
    );
    assert!(session.contains("kappa"), "{session}");
}

#[test]
fn the_agreement_dialog_accepts_other_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Attribute Agreement…"]);
    ui.select("Part:", "batch").expect("part");
    ui.select("Appraiser:", "line").expect("appraiser");
    ui.select("Rating:", "grade").expect("rating");
    ui.click("Run").expect("run");
    assert!(
        ui.session()
            .contains("Attribute Agreement Analysis — batch × line × grade"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_agreement_dialog_rejects_the_same_column_twice() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Attribute Agreement…"]);
    ui.select("Part:", "part").expect("part");
    ui.select("Appraiser:", "part").expect("appraiser");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("three columns must be different"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_gage_dialog_reports_the_crossed_study() {
    let mut ui = Automation::new();
    ui.load_frame(gage_frame(), "gage.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage R&R…"]);
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(
        session.contains("Gage R&R — column measurement"),
        "{session}"
    );
    assert!(session.contains("%Gage R&R"), "{session}");
}

#[test]
fn the_gage_dialog_reads_the_design() {
    let mut ui = Automation::new();
    ui.load_frame(nested_gage_frame(), "nested-gage.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage R&R…"]);
    ui.select("Design:", "nested").expect("design");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Gage R&R — column measurement"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_gage_dialog_rejects_the_part_as_operator() {
    let mut ui = Automation::new();
    ui.load_frame(gage_frame(), "gage.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage R&R…"]);
    ui.select("Operator:", "part").expect("operator");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("three columns must be different"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_type1_dialog_measures_the_bias() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Type 1…"]);
    ui.set_text("Reference:", "20").expect("reference");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Gage Type 1 — column weight"), "{session}");
    assert!(session.contains("Repeatability (6s)"), "{session}");
}

#[test]
fn the_type1_dialog_reads_the_specification_limits() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Type 1…"]);
    ui.set_text("Reference:", "25").expect("reference");
    ui.set_text("LSL:", "20").expect("lsl");
    ui.set_text("USL:", "30").expect("usl");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("%Tolerance (repeatability)"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_type1_dialog_rejects_a_non_numeric_reference() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Gage Type 1…"]);
    ui.set_text("Reference:", "x").expect("reference");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Reference value must be a number"),
        "{}",
        ui.status()
    );
}

#[test]
fn the_control_chart_dialog_labels_the_sensitivity_values() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    ui.select("Column:", "height").expect("column");
    ui.click("2").expect("test 2");
    ui.click("Sensitivity (DEFTEST)").expect("expand");
    assert!(
        ui.unlabeled_widgets().is_empty(),
        "{:?}",
        ui.unlabeled_widgets()
    );
    ui.set_text("K2", "5").expect("sensitivity");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("same side of the center line"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_stat_probability_plot_menu_opens_the_same_dialog() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Basic Statistics", "Probability Plot…"]);
    assert_eq!(ui.window_titles(), vec!["Probability Plot"]);
    ui.press_key(egui::Key::Escape);
    assert!(ui.window_titles().is_empty());
}

#[test]
fn the_taguchi_design_dialog_generates_every_array() {
    for (runs, factors) in [(9, 3), (12, 2), (16, 2), (18, 2)] {
        let mut ui = Automation::new();
        ui.load_frame(frame(), "weights.csv");
        open(&mut ui, &["Stat", "Multivariate", "Taguchi Design…"]);
        ui.click(&format!("L{runs}")).expect("array");
        ui.set_text("Factors:", &factors.to_string())
            .expect("factors");
        ui.click("Generate").expect("generate");
        assert!(
            ui.session()
                .contains(&format!("Taguchi design generated: L{runs}")),
            "L{runs}: {}",
            ui.session()
        );
        assert_eq!(ui.app().data().expect("frame").n_rows(), runs);
    }
}

#[test]
fn the_taguchi_analysis_dialog_reads_the_nominal_quality() {
    let mut ui = Automation::new();
    ui.load_frame(taguchi_frame(), "taguchi.csv");
    open(&mut ui, &["Stat", "Multivariate", "Taguchi Analysis…"]);
    ui.select("Response:", "y").expect("response");
    ui.set_text("Factors:", "A,B").expect("factors");
    ui.click("Nominal is best").expect("quality");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("nominal is best"), "{}", ui.session());
}

#[test]
fn the_sampling_dialog_reads_both_risks() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Multivariate", "Acceptance Sampling…"]);
    ui.click("Plan a scheme").expect("plan");
    ui.set_text("AQL %:", "2").expect("aql");
    ui.set_text("LTPD %:", "8").expect("ltpd");
    ui.set_text("Producer risk %:", "1").expect("producer");
    ui.set_text("Consumer risk %:", "5").expect("consumer");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("AQL 2.00%"), "{session}");
    assert!(session.contains("producer risk 1%"), "{session}");
    assert!(session.contains("consumer risk 5%"), "{session}");
}

#[test]
fn the_create_design_dialog_reads_the_block_size_and_seed() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Create 2-Level Design…"]);
    ui.set_text("Block size:", "4").expect("block");
    ui.click("Randomize run order").expect("randomize");
    ui.set_text("Seed:", "7").expect("seed");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session().contains("DOE design generated: 8 runs"),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("frame").n_rows(), 8);
}

#[test]
fn the_plackett_burman_dialog_reads_the_seed() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Plackett-Burman Design…"]);
    ui.click("Randomize run order").expect("randomize");
    ui.set_text("Seed:", "7").expect("seed");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Plackett-Burman design generated: 12 runs"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_box_behnken_dialog_reads_the_seed() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "DOE", "Box-Behnken Design…"]);
    ui.click("Randomize run order").expect("randomize");
    ui.set_text("Seed:", "7").expect("seed");
    ui.click("Generate").expect("generate");
    assert!(
        ui.session()
            .contains("Box-Behnken design generated: 15 runs"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_alt_dialog_fits_the_exponential_eyring_model() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.click("Exponential").expect("life");
    ui.click("Eyring").expect("acceleration");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Exponential"), "{session}");
    assert!(session.contains("Eyring"), "{session}");
}

#[test]
fn the_cusum_chart_dialog_reads_k_and_h() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    ui.select("Chart:", "CUSUM").expect("chart");
    ui.set_text("k:", "0.5").expect("k");
    ui.set_text("h:", "4").expect("h");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Control Chart: CUSUM"), "{session}");
    assert!(session.contains("0.5"), "{session}");
    assert!(session.contains("4"), "{session}");
}

#[test]
fn the_control_chart_dialog_enables_every_test() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "I–MR…"]);
    for test in 2..=8 {
        ui.click(&test.to_string()).expect("test");
    }
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("Control Chart: I–MR"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_attribute_chart_dialog_draws_every_kind() {
    for kind in [
        "P",
        "NP",
        "C",
        "U",
        "P\u{2032}",
        "NP\u{2032}",
        "C\u{2032}",
        "U\u{2032}",
    ] {
        let mut ui = Automation::new();
        ui.load_frame(frame(), "weights.csv");
        open(&mut ui, &["Stat", "Control Charts", "P/NP/C/U Chart…"]);
        ui.select("Chart:", kind).expect("chart");
        ui.select("Defects column:", "defects").expect("defects");
        ui.click("Run").expect("run");
        assert!(
            ui.session()
                .contains(&format!("{kind} Chart — column defects")),
            "{kind}: {}",
            ui.session()
        );
    }
}

#[test]
fn the_capability_dialog_fits_every_distribution() {
    for distribution in [
        "Normal",
        "Lognormal",
        "Weibull",
        "Exponential",
        "Logistic",
        "Loglogistic",
        "Gamma",
        "Extreme Value",
        "Weibull (3-parameter)",
        "Lognormal (3-parameter)",
    ] {
        let mut ui = Automation::new();
        ui.load_frame(capability_frame(), "capability.csv");
        open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
        ui.select("Distribution:", distribution)
            .expect("distribution");
        ui.set_text("LSL:", "1").expect("lsl");
        ui.click("OK").expect("ok");
        assert!(
            ui.session()
                .contains(&format!("Capability Analysis: {distribution}")),
            "{distribution}: {}",
            ui.session()
        );
    }
}

#[test]
fn the_capability_dialog_reads_box_cox_and_the_target() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.click("Box-Cox transformation").expect("box cox");
    ui.set_text("Target:", "25").expect("target");
    ui.set_text("LSL:", "20").expect("lsl");
    ui.set_text("USL:", "30").expect("usl");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Box-Cox"), "{session}");
    assert!(session.contains("Cpm"), "{session}");
}

#[test]
fn the_capability_dialog_reads_the_bootstrap_and_stability() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.select("Distribution:", "Weibull").expect("distribution");
    ui.set_text("LSL:", "20").expect("lsl");
    ui.set_text("Bootstrap reps:", "50").expect("bootstrap");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(
        session.contains("Capability Analysis: Weibull"),
        "{session}"
    );
    assert!(session.contains("PPM"), "{session}");
}

#[test]
fn the_probability_plot_dialog_reads_the_grouping() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.select("Group:", "batch").expect("group");
    ui.click("OK").expect("ok");
    assert!(
        ui.session().contains("grouped by batch"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_probability_plot_dialog_reads_the_layout() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.click("height").expect("second Y");
    ui.select("Layout:", "Overlay").expect("layout");
    ui.click("OK").expect("ok");
    assert!(ui.session().contains("Overlay"), "{}", ui.session());
}

#[test]
fn the_probability_plot_dialog_reads_the_level_and_position() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.set_text("Level:", "90").expect("level");
    ui.click("OK").expect("ok");
    assert!(ui.session().contains("90% CI"), "{}", ui.session());
}

#[test]
fn the_probability_plot_dialog_reads_the_common_scales() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.click("height").expect("second Y");
    ui.click("Same X scale").expect("same X");
    ui.click("Same Y scale").expect("same Y");
    ui.click("OK").expect("ok");
    assert!(ui.session().contains("same X/Y"), "{}", ui.session());
}

#[test]
fn the_capability_dialog_reads_the_subgroup_and_confidence() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.set_text("LSL:", "20").expect("lsl");
    ui.set_text("USL:", "30").expect("usl");
    ui.set_text("Subgroup size:", "4").expect("subgroup");
    ui.set_text("Confidence:", "90").expect("confidence");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Subgroup size         4"), "{session}");
    assert!(session.contains("90%"), "{session}");
}

#[test]
fn the_capability_dialog_requires_a_stable_screen() {
    let mut ui = Automation::new();
    ui.load_frame(unstable_frame(), "unstable.csv");
    open(&mut ui, &["Stat", "Quality Tools", "Capability Analysis…"]);
    ui.set_text("LSL:", "1").expect("lsl");
    ui.click("Require a stable I–MR screen").expect("stability");
    ui.click("OK").expect("ok");
    assert!(
        ui.status().contains("special-cause signal"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

#[test]
fn the_probability_plot_dialog_suppresses_the_band_for_symbols() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Graph", "Probability Plot…"]);
    ui.select("Display:", "Symbols only").expect("display");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("Symbols only"), "{session}");
    assert!(session.contains("CI off"), "{session}");
    assert!(ui.plot().expect("plot").bands.is_empty());
}

#[test]
fn the_response_surface_dialog_draws_the_steepest_ascent() {
    let mut ui = Automation::new();
    ui.load_frame(rsm_frame(), "rsm.csv");
    open(&mut ui, &["Stat", "DOE", "Response Surface…"]);
    ui.click("Steepest ascent path").expect("ascent");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Steepest ascent"), "{session}");
    assert!(session.contains("Predicted"), "{session}");
    assert!(session.contains("Step"), "{session}");
}

#[test]
fn the_response_surface_dialog_reports_a_flat_surface() {
    let mut ui = Automation::new();
    ui.load_frame(flat_rsm_frame(), "flat.csv");
    open(&mut ui, &["Stat", "DOE", "Response Surface…"]);
    ui.click("Steepest ascent path").expect("ascent");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("no gradient"), "{}", ui.status());
    assert!(ui.session().is_empty(), "{}", ui.session());
}

#[test]
fn the_distribution_comparison_fits_the_box_cox_normal() {
    let mut ui = Automation::new();
    ui.load_frame(capability_frame(), "capability.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    let session = ui.session();
    assert!(session.contains("Box-Cox Normal"), "{session}");
    assert!(session.contains("λ="), "{session}");
}

#[test]
fn the_distribution_comparison_fits_the_johnson_family() {
    let mut ui = Automation::new();
    ui.load_frame(capability_frame(), "capability.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    assert!(ui.session().contains("Johnson S"), "{}", ui.session());
}

#[test]
fn the_xbar_r_dialog_reads_the_subgroup_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–R…"]);
    ui.select("Subgroup column:", "part").expect("column");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("subgroup column part"), "{session}");
    assert!(session.contains("variable sizes"), "{session}");
    assert!(session.contains("Subgroup sizes"), "{session}");
}

#[test]
fn the_xbar_s_dialog_reads_the_subgroup_column() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–S…"]);
    ui.select("Subgroup column:", "part").expect("column");
    ui.click("OK").expect("ok");
    let session = ui.session();
    assert!(session.contains("subgroup column part"), "{session}");
    assert!(session.contains("Subgroup sizes"), "{session}");
}

#[test]
fn the_subgroup_column_rejects_singleton_subgroups() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    open(&mut ui, &["Stat", "Control Charts", "X̄–R…"]);
    ui.select("Subgroup column:", "weight").expect("column");
    ui.click("OK").expect("ok");
    assert!(
        ui.status().contains("outside the supported range"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

/// Two responses with a common interior optimum, for the response optimizer.
fn desirability_frame() -> DataFrame {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y1 = Vec::new();
    let mut y2 = Vec::new();
    for first in [-1.0_f64, 0.0, 1.0] {
        for second in [-1.0_f64, 0.0, 1.0] {
            a.push(first);
            b.push(second);
            y1.push(10.0 - (first - 0.3).powi(2) - (second + 0.2).powi(2));
            y2.push(5.0 + (first - 0.3).powi(2) + (second + 0.2).powi(2));
        }
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y1", y1),
        Column::new_f64("y2", y2),
    ])
    .expect("desirability frame")
}

#[test]
fn the_response_optimizer_combines_two_responses() {
    let mut ui = Automation::new();
    ui.load_frame(desirability_frame(), "desirability.csv");
    open(&mut ui, &["Stat", "DOE", "Response Optimizer…"]);
    ui.click("A").expect("factor A");
    ui.click("B").expect("factor B");
    ui.select("Add response:", "Add y1").expect("y1");
    ui.select("Add response:", "Add y2").expect("y2");
    for (response, goal) in [("y1", "Maximize"), ("y2", "Minimize")] {
        ui.select(&format!("Goal ({response}):"), goal)
            .expect("goal");
        ui.set_text(&format!("Low ({response}):"), "5")
            .expect("low");
        ui.set_text(&format!("High ({response}):"), "10")
            .expect("high");
    }
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Response Optimizer"), "{session}");
    assert!(session.contains("Combined desirability"), "{session}");
    assert!(session.contains("0.300000"), "{session}");
    assert!(session.contains("1.000000"), "{session}");
}

#[test]
fn the_response_optimizer_reads_a_target_goal() {
    let mut ui = Automation::new();
    ui.load_frame(desirability_frame(), "desirability.csv");
    open(&mut ui, &["Stat", "DOE", "Response Optimizer…"]);
    ui.click("A").expect("factor A");
    ui.click("B").expect("factor B");
    ui.select("Add response:", "Add y1").expect("y1");
    ui.select("Goal (y1):", "Target").expect("goal");
    ui.set_text("Low (y1):", "5").expect("low");
    ui.set_text("Target (y1):", "10").expect("target");
    ui.set_text("High (y1):", "10.5").expect("high");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Target"), "{session}");
    assert!(session.contains("Combined desirability"), "{session}");
}

#[test]
fn the_response_optimizer_needs_a_response() {
    let mut ui = Automation::new();
    ui.load_frame(desirability_frame(), "desirability.csv");
    open(&mut ui, &["Stat", "DOE", "Response Optimizer…"]);
    ui.click("A").expect("factor A");
    ui.click("B").expect("factor B");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("Add at least one response"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

/// Two failure modes at two stress levels, for the competing-risks ALT.
fn competing_frame() -> DataFrame {
    let mut time = Vec::new();
    let mut stress = Vec::new();
    let mut mode = Vec::new();
    for index in 0..120 {
        let level: f64 = if index % 2 == 0 { 300.0 } else { 350.0 };
        let life = (7.0 - 0.02 * level).exp();
        let first = ((index * 37 % 97) as f64 + 0.5) / 97.0;
        let second = ((index * 53 % 89) as f64 + 0.5) / 89.0;
        let a = life * (-(1.0 - first).ln()).powf(0.5);
        let b = 1.6 * life * (-(1.0 - second).ln()).powf(1.0 / 1.5);
        if a < b {
            time.push(a);
            mode.push("A".to_owned());
        } else {
            time.push(b);
            mode.push("B".to_owned());
        }
        stress.push(level);
    }
    DataFrame::new(vec![
        Column::new_f64("time", time),
        Column::new_f64("stress", stress),
        Column::new_text("mode", mode),
        Column::new_text("site", vec!["S1".to_owned(); 120]),
    ])
    .expect("competing frame")
}

#[test]
fn the_alt_dialog_fits_competing_risks() {
    let mut ui = Automation::new();
    ui.load_frame(competing_frame(), "competing.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.select("Mode column:", "mode").expect("mode");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("competing risks"), "{session}");
    assert!(session.contains("CIF at horizon"), "{session}");
    assert!(session.contains("B10"), "{session}");
}

#[test]
fn the_alt_dialog_reads_the_competing_horizon() {
    let mut ui = Automation::new();
    ui.load_frame(competing_frame(), "competing.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.select("Mode column:", "mode").expect("mode");
    ui.set_text("Horizon:", "100").expect("horizon");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Horizon"), "{session}");
    assert!(session.contains("100.000000"), "{session}");
}

#[test]
fn the_alt_dialog_rejects_a_single_failure_mode() {
    let mut ui = Automation::new();
    ui.load_frame(competing_frame(), "competing.csv");
    open(
        &mut ui,
        &["Stat", "Reliability", "Accelerated Life Testing…"],
    );
    ui.select("Mode column:", "site").expect("mode");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least two failure modes"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

/// A two-step stress test with a Weibull/Arrhenius truth.
fn step_stress_frame() -> DataFrame {
    let shape = 1.5_f64;
    let intercept = 200.0_f64.ln();
    let slope = 3000.0;
    let centre = 1.0 / 325.0;
    let eta = |stress: f64| (intercept + slope * (1.0 / stress - centre)).exp();
    let eta1 = eta(300.0);
    let eta2 = eta(350.0);
    let mut generator = fp_core::special::SplitMix64::new(20261029);
    let mut unit = Vec::new();
    let mut duration = Vec::new();
    let mut stress = Vec::new();
    let mut event = Vec::new();
    for index in 0..200 {
        let u = generator.next_f64().clamp(1e-12, 1.0 - 1e-12);
        let exposure = (-(1.0 - u).ln()).powf(1.0 / shape);
        let first = 20.0 / eta1;
        let label = format!("U{index}");
        if exposure <= first {
            unit.push(label);
            duration.push(exposure * eta1);
            stress.push(300.0);
            event.push(1.0);
        } else {
            let second = (exposure - first) * eta2;
            unit.push(label.clone());
            duration.push(20.0);
            stress.push(300.0);
            event.push(0.0);
            unit.push(label);
            stress.push(350.0);
            if second <= 40.0 {
                duration.push(second);
                event.push(1.0);
            } else {
                duration.push(40.0);
                event.push(0.0);
            }
        }
    }
    DataFrame::new(vec![
        Column::new_text("unit", unit),
        Column::new_f64("duration", duration),
        Column::new_f64("stress", stress),
        Column::new_f64("event", event),
    ])
    .expect("step-stress frame")
}

#[test]
fn the_step_stress_dialog_fits_the_cumulative_exposure() {
    let mut ui = Automation::new();
    ui.load_frame(step_stress_frame(), "stepstress.csv");
    open(&mut ui, &["Stat", "Reliability", "Step-Stress ALT…"]);
    ui.select("Duration:", "duration").expect("duration");
    ui.select("Stress:", "stress").expect("stress");
    ui.select("Unit:", "unit").expect("unit");
    ui.select("Event:", "event").expect("event");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Step-Stress ALT"), "{session}");
    assert!(session.contains("Life at use"), "{session}");
    assert!(session.contains("Shape (Weibull)"), "{session}");
    assert!(ui.plot().is_some(), "a plot was produced");
}

#[test]
fn the_step_stress_dialog_reads_the_exponential_life() {
    let mut ui = Automation::new();
    ui.load_frame(step_stress_frame(), "stepstress.csv");
    open(&mut ui, &["Stat", "Reliability", "Step-Stress ALT…"]);
    ui.select("Duration:", "duration").expect("duration");
    ui.select("Stress:", "stress").expect("stress");
    ui.select("Unit:", "unit").expect("unit");
    ui.select("Event:", "event").expect("event");
    ui.click("Exponential").expect("life");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Exponential"), "{}", ui.session());
    assert!(
        !ui.session().contains("Shape (Weibull)"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_step_stress_dialog_rejects_a_failure_before_the_last_step() {
    let mut ui = Automation::new();
    ui.load_frame(step_stress_frame(), "stepstress.csv");
    open(&mut ui, &["Stat", "Reliability", "Step-Stress ALT…"]);
    ui.select("Duration:", "duration").expect("duration");
    ui.select("Stress:", "stress").expect("stress");
    ui.select("Unit:", "unit").expect("unit");
    ui.select("Event:", "event").expect("event");
    // The first row of U0 is a complete step in this fixture; use the stress
    // column as the event so the values are not 0/1.
    ui.select("Event:", "stress").expect("event");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("must contain 0 or 1"),
        "{}",
        ui.status()
    );
    assert!(ui.session().is_empty(), "{}", ui.session());
}

fn date_frame() -> DataFrame {
    let date = |text: &str| fp_core::data::parse_date(text).expect("date");
    DataFrame::new(vec![
        Column::new_date(
            "start",
            vec![date("2024-01-01"), date("2024-02-29"), date("2025-01-01")],
        ),
        Column::new_date(
            "end",
            vec![date("2024-03-01"), date("2025-01-01"), date("2024-01-01")],
        ),
        Column::new_f64("value", vec![1.0, 2.0, 3.0]),
    ])
    .expect("date frame")
}

#[test]
fn the_date_difference_dialog_adds_the_days() {
    let mut ui = Automation::new();
    ui.load_frame(date_frame(), "dates.csv");
    open(&mut ui, &["Data", "Date Difference…"]);
    ui.select("First date:", "start").expect("first");
    ui.select("Second date:", "end").expect("second");
    ui.set_text("New column:", "days").expect("name");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("data");
    let days = data.column("days").expect("days");
    assert_eq!(days.value(0), Some(60.0));
    assert_eq!(days.value(1), Some(307.0));
    assert_eq!(days.value(2), Some(-366.0));
}

#[test]
fn the_add_days_dialog_shifts_across_a_leap_year() {
    let mut ui = Automation::new();
    ui.load_frame(date_frame(), "dates.csv");
    open(&mut ui, &["Data", "Add Days…"]);
    ui.select("Date column:", "start").expect("column");
    ui.set_text("Days:", "59").expect("days");
    ui.set_text("New column:", "later").expect("name");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("data");
    let later = data.column("later").expect("later");
    assert_eq!(later.date(0), fp_core::data::parse_date("2024-02-29"));
    assert_eq!(later.date(1), fp_core::data::parse_date("2024-04-28"));
}

#[test]
fn the_date_dialogs_reject_a_duplicate_name() {
    let mut ui = Automation::new();
    ui.load_frame(date_frame(), "dates.csv");
    open(&mut ui, &["Data", "Add Days…"]);
    ui.select("Date column:", "start").expect("column");
    ui.set_text("New column:", "start").expect("name");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("already exists"), "{}", ui.status());
    assert!(ui.app().data().expect("data").column("start").is_ok());
}

#[test]
fn the_distribution_comparison_reports_the_best_fit() {
    let mut ui = Automation::new();
    ui.load_frame(capability_frame(), "capability.csv");
    open(
        &mut ui,
        &["Stat", "Quality Tools", "Distribution Comparison…"],
    );
    let session = ui.session();
    assert!(session.contains("Best fit:"), "{session}");
    assert!(session.contains("Selection rule"), "{session}");
}
