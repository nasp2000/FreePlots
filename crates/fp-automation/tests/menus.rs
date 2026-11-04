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
fn the_graph_actions_live_in_the_plot_context_menu() {
    let mut ui = Automation::new();
    ui.load_csv(fixture()).expect("load csv");
    // Keep the graph inside the main window so its context menu is reachable.
    ui.app_mut().set_native_plot_windows(false);
    ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])
        .expect("open the attribute dialog");
    ui.select("Chart:", "P").expect("select the P chart");
    ui.select("Defects column:", "defects")
        .expect("select the column");
    ui.set_text("Constant size:", "50").expect("set the size");
    ui.click("Run").expect("run the chart");
    let plot_title = ui.plot().expect("the chart produced no plot").title.clone();
    assert!(
        ui.window_titles()
            .iter()
            .any(|window| window == &plot_title)
    );

    // The Graph menu no longer repeats the presentation edits.
    ui.open_menu(&["Graph"]).expect("open the Graph menu");
    let labels: Vec<String> = ui.widgets().into_iter().filter_map(|w| w.label).collect();
    for gone in [
        "Edit graph labels…",
        "Add reference line…",
        "Save graph…",
        "Copy graph as SVG",
        "Reset graph edits",
    ] {
        assert!(
            !labels.iter().any(|label| label == gone),
            "{gone} is still in the Graph menu: {labels:?}"
        );
    }
    assert!(
        labels.iter().any(|label| label == "Pareto Chart…"),
        "the plot entries stay in the menu: {labels:?}"
    );
    // Close the menu again by clicking its title (Escape would also close the
    // graph dialog underneath).
    ui.click("Graph").expect("close the Graph menu");

    // Right-clicking the graph opens the same edits, and they still work.
    let rect = ui
        .widgets()
        .into_iter()
        .find(|widget| widget.role == "Window" && widget.label.as_deref() == Some(&plot_title))
        .map(|widget| widget.rect)
        .expect("the graph window rect");
    let centre = egui::pos2((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
    ui.right_click_at(centre);
    let menu: Vec<String> = ui.widgets().into_iter().filter_map(|w| w.label).collect();
    assert!(
        menu.iter().any(|label| label == "Edit graph labels…"),
        "the context menu did not open: {menu:?}"
    );
    ui.click("Edit graph labels…").expect("open the editor");
    ui.set_text("Subtitle", "Night shift")
        .expect("set the subtitle");
    ui.click("Apply").expect("apply the labels");
    assert!(
        ui.status().contains("Graph labels updated"),
        "{}",
        ui.status()
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
