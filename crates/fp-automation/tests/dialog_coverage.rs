// SPDX-License-Identifier: MIT
//! Every dialog item of the Stat, Graph and Calc menus opens a window.

use fp_automation::Automation;
use fp_core::data::{Column, DataFrame};

/// Menu paths whose last item opens a dialog window.
const DIALOG_MENUS: &[&[&str]] = &[
    &["Calc", "Calculator…"],
    &["Edit", "Conditional formatting…"],
    &["File", "Batch descriptive statistics…"],
    &["File", "Open CSV…"],
    &["File", "Open Project…"],
    &["File", "Save CSV As…"],
    &["File", "Save Project As…"],
    &["File", "Sample projects…"],
    &["Assistant", "Choose an analysis…"],
    &["Help", "Keyboard shortcuts"],
    &["Graph", "More Graphs", "Bar Chart…"],
    &["Graph", "More Graphs", "Correlogram…"],
    &["Graph", "Dotplot…"],
    &["Graph", "Empirical CDF…"],
    &["Graph", "Matrix Plot…"],
    &["Graph", "Interval Plot…"],
    &["Graph", "Pareto Chart…"],
    &["Graph", "Stem-and-Leaf…"],
    &["Graph", "Time Series Plot…"],
    &[
        "Stat",
        "Basic Statistics",
        "Display Descriptive Statistics by…",
    ],
    &["Stat", "Basic Statistics", "Probability Plot…"],
    &["Stat", "Basic Statistics", "1-Sample t…"],
    &["Stat", "Basic Statistics", "2-Sample t…"],
    &["Stat", "Basic Statistics", "Paired t…"],
    &["Stat", "Basic Statistics", "Equivalence Test…"],
    &["Stat", "Basic Statistics", "Chi-Square Goodness-of-Fit…"],
    &["Stat", "Basic Statistics", "Grubbs Test…"],
    &["Stat", "Basic Statistics", "Power and Sample Size…"],
    &["Stat", "Basic Statistics", "Mann-Whitney…"],
    &["Stat", "Basic Statistics", "Wilcoxon Signed-Rank…"],
    &["Stat", "Basic Statistics", "Kruskal-Wallis…"],
    &["Stat", "Basic Statistics", "Chi-Square Test…"],
    &["Stat", "Basic Statistics", "Two-Way ANOVA…"],
    &["Stat", "Basic Statistics", "Curve Fit…"],
    &["Stat", "Basic Statistics", "Correlation Matrix…"],
    &["Stat", "Basic Statistics", "Tolerance Interval…"],
    &["Stat", "Basic Statistics", "Box-Cox Transformation…"],
    &["Stat", "Basic Statistics", "Partial Correlation…"],
    &["Stat", "Basic Statistics", "Friedman…"],
    &["Stat", "Basic Statistics", "Sign Test…"],
    &["Stat", "Basic Statistics", "Mood's Median Test…"],
    &["Stat", "Basic Statistics", "1 Proportion…"],
    &["Stat", "Basic Statistics", "2 Proportions…"],
    &["Stat", "Basic Statistics", "1 Variance…"],
    &["Stat", "Basic Statistics", "2 Variances…"],
    &["Stat", "Multivariate", "Principal Components…"],
    &["Stat", "Multivariate", "Taguchi Design…"],
    &["Stat", "Multivariate", "Taguchi Analysis…"],
    &["Stat", "Multivariate", "Acceptance Sampling…"],
    &["Stat", "Multivariate", "Time Series Plot…"],
    &["Stat", "Multivariate", "Stem-and-Leaf…"],
    &["Stat", "Multivariate", "Empirical CDF…"],
    &["Stat", "Multivariate", "Dotplot…"],
    &["Stat", "Multivariate", "Matrix Plot…"],
    &["Stat", "Multivariate", "Hotelling T² Chart…"],
    &["Stat", "Multivariate", "Generalized Variance Chart…"],
    &["Stat", "Multivariate", "Multivariate EWMA Chart…"],
    &["Stat", "Multivariate", "K-Means Clustering…"],
    &["Stat", "Time Series", "Moving Average…"],
    &["Stat", "Time Series", "Decomposition…"],
    &["Stat", "Time Series", "Exponential Smoothing…"],
    &["Stat", "Regression", "Regression…"],
    &["Stat", "Regression", "Best Subsets…"],
    &["Stat", "Regression", "Stepwise Regression…"],
    &["Stat", "Regression", "Ordinal Logistic Regression…"],
    &["Stat", "Regression", "Poisson Regression…"],
    &["Stat", "Regression", "Binary Logistic Regression…"],
    &["Stat", "ANOVA", "General Linear Model…"],
    &["Stat", "ANOVA", "One-Way…"],
    &["Stat", "ANOVA", "Levene Test…"],
    &["Stat", "ANOVA", "Bartlett Test…"],
    &["Stat", "DOE", "Create 2-Level Design…"],
    &["Stat", "DOE", "Plackett-Burman Design…"],
    &["Stat", "DOE", "Box-Behnken Design…"],
    &["Stat", "DOE", "2-Level Factorial…"],
    &["Stat", "DOE", "Factorial Plots…"],
    &["Stat", "DOE", "Response Surface…"],
    &["Stat", "DOE", "Response Optimizer…"],
    &["Stat", "Reliability", "Accelerated Life Testing…"],
    &["Stat", "Reliability", "Step-Stress ALT…"],
    &["Stat", "Reliability", "Kaplan–Meier…"],
    &["Stat", "Reliability", "Weibull (censored)…"],
    &["Stat", "Reliability", "Exponential (censored)…"],
    &["Stat", "Reliability", "Lognormal (censored)…"],
    &["Stat", "Control Charts", "I–MR…"],
    &["Stat", "Control Charts", "X̄–R…"],
    &["Stat", "Control Charts", "X̄–S…"],
    &["Stat", "Control Charts", "EWMA…"],
    &["Stat", "Control Charts", "P/NP/C/U Chart…"],
    &["Stat", "Control Charts", "G Chart (Rare Events)…"],
    &["Stat", "Control Charts", "Multivariate Charts…"],
    &["Stat", "Quality Tools", "Capability Analysis…"],
    &["Stat", "Quality Tools", "Distribution Comparison…"],
    &[
        "Stat",
        "Quality Tools",
        "Individual Distribution Identification…",
    ],
    &["Stat", "Quality Tools", "Interval Plot…"],
    &["Stat", "Quality Tools", "Pareto Chart…"],
    &["Stat", "Quality Tools", "Gage Linearity…"],
    &["Stat", "Quality Tools", "Attribute Agreement…"],
    &["Stat", "Quality Tools", "Gage R&R…"],
    &["Stat", "Quality Tools", "Gage Type 1…"],
    &["Graph", "Probability Plot…"],
    &["Graph", "Scatterplot…"],
];

fn frame() -> DataFrame {
    let weight: Vec<f64> = (0..24).map(|index| 20.0 + (index as f64) * 0.35).collect();
    let height: Vec<f64> = (0..24).map(|index| 10.0 + (index as f64) * 0.12).collect();
    let pressure: Vec<f64> = (0..24).map(|index| 100.0 + (index as f64) * 1.5).collect();
    let batch: Vec<String> = (0..24)
        .map(|index| if index % 2 == 0 { "A" } else { "B" }.to_owned())
        .collect();
    DataFrame::new(vec![
        Column::new_f64("weight", weight),
        Column::new_f64("height", height),
        Column::new_f64("pressure", pressure),
        Column::new_text("batch", batch),
    ])
    .expect("frame")
}

#[test]
fn every_dialog_menu_opens_and_closes_a_window() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "coverage.csv");
    for path in DIALOG_MENUS {
        let joined = path.join(" > ");
        let session_before = ui.session().len();
        let status_before = ui.status().to_owned();
        ui.open_menu(path)
            .unwrap_or_else(|error| panic!("{joined}: {error}"));
        let titles = ui.window_titles();
        let reacted = !titles.is_empty()
            || ui.session().len() != session_before
            || ui.status() != status_before;
        assert!(
            reacted,
            "{joined} opened no window and produced no report or status"
        );
        if !titles.is_empty() {
            let unlabeled = ui.unlabeled_widgets();
            assert!(
                unlabeled.is_empty(),
                "{joined} has unlabeled controls: {unlabeled:?}"
            );
            ui.press_key(egui::Key::Escape);
            assert!(
                ui.window_titles().is_empty(),
                "{joined} left {:?} open",
                ui.window_titles()
            );
        }
    }
}
