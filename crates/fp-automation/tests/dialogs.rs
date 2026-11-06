//! End-to-end option tests for the labelled dialogs.

use fp_automation::Automation;
use fp_core::data::{Column, DataFrame};

fn frame() -> DataFrame {
    let weight: Vec<f64> = (0..24)
        .map(|index| 24.0 + (index as f64 % 6.0) * 0.4 - (index as f64 / 6.0) * 0.1)
        .collect();
    let height: Vec<f64> = (0..24).map(|index| 12.0 + (index as f64) * 0.1).collect();
    let batch: Vec<String> = (0..24)
        .map(|index| if index % 2 == 0 { "A" } else { "B" }.to_owned())
        .collect();
    DataFrame::new(vec![
        Column::new_f64("weight", weight),
        Column::new_f64("height", height),
        Column::new_text("batch", batch),
    ])
    .expect("frame")
}

#[test]
fn the_probability_plot_dialog_reaches_the_engine() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Graph", "Probability Plot…"])
        .expect("open the dialog");
    ui.select("Distribution:", "Weibull")
        .expect("select Weibull");
    ui.click("OK").expect("run");
    assert!(
        ui.session().contains("Probability Plot"),
        "{}",
        ui.session()
    );
    assert!(ui.session().contains("Weibull"), "{}", ui.session());
    let plot = ui.plot().expect("plot");
    assert!(!plot.points.is_empty());
}

#[test]
fn the_capability_dialog_reads_the_specification_limits() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Stat", "Quality Tools", "Capability Analysis…"])
        .expect("open the dialog");
    ui.select("Column:", "weight").expect("column");
    ui.set_text("LSL:", "22").expect("LSL");
    ui.set_text("USL:", "26").expect("USL");
    ui.click("OK").expect("run");
    let session = ui.session();
    assert!(session.contains("Capability Analysis"), "{session}");
    assert!(session.contains("Cpk"), "{session}");
    assert!(session.contains("LSL"), "{session}");
}

#[test]
fn the_capability_dialog_reports_a_reversed_specification() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Stat", "Quality Tools", "Capability Analysis…"])
        .expect("open the dialog");
    ui.select("Column:", "weight").expect("column");
    ui.set_text("LSL:", "30").expect("LSL");
    ui.set_text("USL:", "10").expect("USL");
    ui.click("OK").expect("run");
    assert!(
        ui.status().to_lowercase().contains("limit"),
        "{}",
        ui.status()
    );
    assert!(
        !ui.session().contains("Capability Analysis"),
        "{}",
        ui.session()
    );
}

#[test]
fn the_one_sample_t_dialog_reads_the_null_mean() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Stat", "Basic Statistics", "1-Sample t…"])
        .expect("open the dialog");
    ui.set_text("Null mean:", "24").expect("null mean");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("1-Sample t"), "{}", ui.session());
    assert!(ui.session().contains("Null mean"), "{}", ui.session());
}

#[test]
fn the_scatterplot_dialog_reads_both_columns() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Graph", "Scatterplot…"])
        .expect("open the dialog");
    ui.select("X:", "weight").expect("X");
    ui.select("Y:", "height").expect("Y");
    ui.click("Run").expect("run");
    assert!(ui.session().contains("Scatterplot"), "{}", ui.session());
    assert!(ui.session().contains("Pearson"), "{}", ui.session());
}

#[test]
fn the_regression_dialog_reads_the_selected_predictor() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Stat", "Regression", "Regression…"])
        .expect("open the dialog");
    // The dialog starts with every column except the response selected, so a
    // click removes the predictor from the model.
    ui.click("height").expect("deselect the predictor");
    ui.click("Run").expect("run");
    let session = ui.session();
    assert!(session.contains("Regression"), "{session}");
    assert!(!session.contains("height"), "{session}");
    assert!(session.contains("batch"), "{session}");
}

#[test]
fn the_ewma_dialog_reads_the_smoothing_parameter() {
    let mut ui = Automation::new();
    ui.load_frame(frame(), "weights.csv");
    ui.open_menu(&["Stat", "Control Charts", "EWMA…"])
        .expect("open the dialog");
    ui.set_text("λ:", "0.3").expect("lambda");
    ui.click("OK").expect("run");
    assert!(ui.session().contains("EWMA"), "{}", ui.session());
    assert!(ui.session().contains("0.3"), "{}", ui.session());
}
