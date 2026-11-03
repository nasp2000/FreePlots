//! End-to-end UI automation tests: menus, dialogs and reports.

use fp_automation::Automation;

fn fixture() -> String {
    format!("{}/tests/data/pchart.csv", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_attribute_dialog_runs_through_the_real_widgets() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])
        .expect("open the attribute dialog");
    let labels: Vec<String> = ui
        .widgets()
        .into_iter()
        .filter_map(|widget| widget.label)
        .collect();
    assert!(labels.iter().any(|label| label == "Chart:"), "{labels:?}");
    assert!(
        labels.iter().any(|label| label == "Defects column:"),
        "{labels:?}"
    );
    assert!(labels.iter().any(|label| label == "Run"), "{labels:?}");

    ui.select("Chart:", "P").expect("select the P chart");
    ui.select("Defects column:", "defects")
        .expect("select the column");
    ui.set_text("Constant size:", "50").expect("set the size");
    ui.click("Run").expect("run the chart");

    assert!(ui.session().contains("P Chart"), "{}", ui.session());
    assert!(ui.session().contains("Center line"), "{}", ui.session());
    assert!(ui.plot().is_some(), "the chart produced no plot");
}

#[test]
fn an_invalid_option_is_reported_in_the_status_bar() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])
        .expect("open the attribute dialog");
    ui.select("Chart:", "P").expect("select the P chart");
    ui.select("Defects column:", "defects")
        .expect("select the column");
    ui.set_text("Constant size:", "0").expect("set the size");
    ui.click("Run").expect("click Run");
    assert!(ui.status().contains("positive integer"), "{}", ui.status());
    assert!(!ui.session().contains("P Chart"), "{}", ui.session());
}

#[test]
fn the_graph_menu_edits_and_saves_the_plot() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])
        .expect("open the attribute dialog");
    ui.select("Chart:", "P").expect("select the P chart");
    ui.select("Defects column:", "defects")
        .expect("select the column");
    ui.set_text("Constant size:", "50").expect("set the size");
    ui.click("Run").expect("run the chart");
    assert!(ui.plot().is_some(), "the chart produced no plot");

    // The Graph menu edits the plot through the same dialog the window uses.
    ui.open_menu(&["Graph", "Edit graph labels…"])
        .expect("open the label editor");
    ui.set_text("Subtitle", "Night shift")
        .expect("set the subtitle");
    ui.click("Apply").expect("apply the labels");
    assert!(
        ui.status().contains("Graph labels updated"),
        "{}",
        ui.status()
    );

    // Saving the graph asks for an SVG destination.
    ui.open_menu(&["Graph", "Save graph…"])
        .expect("open the save dialog");
    assert!(
        ui.window_titles()
            .iter()
            .any(|title| title == "Save Graph As"),
        "the SVG save dialog did not open: {:?}",
        ui.window_titles()
    );
    ui.click("Cancel").expect("cancel the save");
    assert!(
        !ui.window_titles()
            .iter()
            .any(|title| title == "Save Graph As"),
        "the dialog stayed open: {:?}",
        ui.window_titles()
    );
}

#[test]
fn the_widget_tree_exposes_the_menu_and_toolbar() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    let labels: Vec<String> = ui
        .widgets()
        .into_iter()
        .filter_map(|widget| widget.label)
        .collect();
    for expected in ["File", "Edit", "Data", "Stat", "Graph", "Tools", "Help"] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing menu {expected}: {labels:?}"
        );
    }
    assert!(
        labels.iter().any(|label| label == "Open CSV"),
        "missing the toolbar: {labels:?}"
    );
}

/// The shipped build has no GPU/PNG stack: the live bridge takes screenshots.
#[cfg(feature = "wgpu-render")]
#[test]
fn the_current_frame_can_be_rendered_to_png() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    ui.open_menu(&["Help", "About FreePlots"])
        .expect("open About");
    let path =
        std::env::temp_dir().join(format!("freeplots-automation-{}.png", std::process::id()));
    ui.screenshot(&path).expect("screenshot");
    let bytes = std::fs::read(&path).expect("read png");
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']), "not a PNG");
    assert!(bytes.len() > 1000, "the PNG is suspiciously small");
    let _ = std::fs::remove_file(&path);
}
