//! Native coverage of every menu item: Data, Edit, File, Stat, Graph, Calc,
//! Tools, Window and Help.

use fp_automation::Automation;
use fp_core::data::{Column, DataFrame};

fn full_frame() -> DataFrame {
    let n = 24;
    let mut weight = Vec::new();
    let mut height = Vec::new();
    let mut pressure = Vec::new();
    let mut pass = Vec::new();
    let mut pass2 = Vec::new();
    let mut defects = Vec::new();
    let mut grade = Vec::new();
    let mut batch = Vec::new();
    let mut line = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    let mut time = Vec::new();
    let mut censor = Vec::new();
    let mut stress = Vec::new();
    let mut mode = Vec::new();
    for index in 0..n {
        let noise = ((index * 37 % 17) as f64 - 8.0) * 0.03;
        weight.push(20.0 + 0.15 * (10.0 + index as f64 * 0.1) + noise);
        height.push(10.0 + index as f64 * 0.1);
        pressure.push(100.0 + (index % 6) as f64 * 2.0);
        pass.push((index % 2) as f64);
        pass2.push(((index + 1) % 2) as f64);
        defects.push((index % 4) as f64);
        grade.push((index % 3 + 1) as f64);
        batch.push(if index % 2 == 0 { "A" } else { "B" }.to_owned());
        line.push(if (index / 2) % 2 == 0 { "L1" } else { "L2" }.to_owned());
        part.push(format!("P{}", index % 6 + 1));
        operator.push(if (index / 3) % 2 == 0 { "O1" } else { "O2" }.to_owned());
        time.push(2.0 + index as f64 * 0.5);
        censor.push(if index % 4 == 0 { 1.0 } else { 0.0 });
        stress.push(if index % 2 == 0 { 300.0 } else { 350.0 });
        mode.push(if index % 3 == 0 { "A" } else { "B" }.to_owned());
    }
    DataFrame::new(vec![
        Column::new_f64("weight", weight),
        Column::new_f64("height", height),
        Column::new_f64("pressure", pressure),
        Column::new_f64("pass", pass),
        Column::new_f64("pass2", pass2),
        Column::new_f64("defects", defects),
        Column::new_f64("grade", grade),
        Column::new_text("batch", batch),
        Column::new_text("line", line),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
        Column::new_f64("time", time),
        Column::new_f64("censor", censor),
        Column::new_f64("stress", stress),
        Column::new_text("mode", mode),
        Column::new_f64(
            "A",
            (0..n)
                .map(|i| if i % 2 == 0 { -1.0 } else { 1.0 })
                .collect(),
        ),
        Column::new_f64(
            "B",
            (0..n)
                .map(|i| if (i / 2) % 2 == 0 { -1.0 } else { 1.0 })
                .collect(),
        ),
        Column::new_f64(
            "C",
            (0..n)
                .map(|i| if (i / 4) % 2 == 0 { -1.0 } else { 1.0 })
                .collect(),
        ),
    ])
    .expect("frame")
}

const MENU_PATHS: &[&[&str]] = &[
    &["Edit", "Undo"],
    &["Edit", "Redo"],
    &["Edit", "Clear Cells"],
    &["Edit", "Rename Column…"],
    &["Edit", "Conditional formatting…"],
    &["Data", "Add Row"],
    &["Data", "Insert Row Above"],
    &["Data", "Delete Row"],
    &["Data", "Add Numeric Column"],
    &["Data", "Add Text Column"],
    &["Data", "Add Date Column"],
    &["Data", "Delete Column"],
    &["Data", "Sort…"],
    &["Data", "Transpose"],
    &["Data", "Stack…"],
    &["Data", "Unstack…"],
    &["Data", "Subset…"],
    &["Data", "Date Difference…"],
    &["Data", "Add Days…"],
    &["Calc", "Calculator…"],
    &["Stat", "Basic Statistics", "Display Descriptive Statistics"],
    &[
        "Stat",
        "Basic Statistics",
        "Display Descriptive Statistics by…",
    ],
    &["Stat", "Basic Statistics", "Probability Plot…"],
    &["Stat", "Basic Statistics", "Normality Test (AD)"],
    &["Stat", "Basic Statistics", "D'Agostino–Pearson Test"],
    &["Stat", "Basic Statistics", "Kolmogorov–Smirnov Test"],
    &["Stat", "Basic Statistics", "Ryan–Joiner Test"],
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
    &["Stat", "Multivariate", "K-Means Clustering…"],
    &["Stat", "Time Series", "Moving Average…"],
    &["Stat", "Time Series", "Decomposition…"],
    &["Stat", "Time Series", "Exponential Smoothing…"],
    &["Stat", "Regression", "Regression…"],
    &["Stat", "Regression", "Stepwise Regression…"],
    &["Stat", "Regression", "Ordinal Logistic Regression…"],
    &["Stat", "Regression", "Poisson Regression…"],
    &["Stat", "Regression", "Binary Logistic Regression…"],
    &["Stat", "ANOVA", "One-Way…"],
    &["Stat", "ANOVA", "Levene Test…"],
    &["Stat", "ANOVA", "Bartlett Test…"],
    &["Stat", "DOE", "Create 2-Level Design…"],
    &["Stat", "DOE", "Plackett-Burman Design…"],
    &["Stat", "DOE", "Box-Behnken Design…"],
    &["Stat", "DOE", "2-Level Factorial…"],
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
    &["Stat", "Control Charts", "Run Chart"],
    &["Stat", "Control Charts", "P/NP/C/U Chart…"],
    &["Stat", "Control Charts", "G Chart (Rare Events)…"],
    &["Stat", "Quality Tools", "Capability Analysis…"],
    &["Stat", "Quality Tools", "Distribution Comparison…"],
    &["Stat", "Quality Tools", "Interval Plot…"],
    &["Stat", "Quality Tools", "Pareto Chart…"],
    &["Stat", "Quality Tools", "Gage Linearity…"],
    &["Stat", "Quality Tools", "Attribute Agreement…"],
    &["Stat", "Quality Tools", "Gage R&R…"],
    &["Stat", "Quality Tools", "Gage Type 1…"],
    &["Graph", "Probability Plot…"],
    &["Graph", "Boxplot"],
    &["Graph", "Histogram"],
    &["Graph", "Scatterplot…"],
];

fn shape(ui: &Automation) -> Option<(usize, usize)> {
    ui.app().data().map(|df| (df.n_rows(), df.n_columns()))
}

#[test]
fn every_menu_item_changes_the_interface() {
    let mut failures = Vec::new();
    for path in MENU_PATHS {
        let mut ui = Automation::new();
        ui.load_frame(full_frame(), "full.csv");
        let before_status = ui.status().to_owned();
        let before_session = ui.session().to_owned();
        let before_windows = ui.window_titles();
        let before_selected = ui.app().selected_column().map(str::to_owned);
        let before_shape = shape(&ui);
        ui.open_menu(path)
            .unwrap_or_else(|error| panic!("open {path:?}: {error}"));
        let changed = ui.status() != before_status
            || ui.session() != before_session
            || ui.window_titles() != before_windows
            || ui.app().selected_column().map(str::to_owned) != before_selected
            || shape(&ui) != before_shape;
        if !changed {
            failures.push(path.join(" > "));
        }
    }
    assert!(
        failures.is_empty(),
        "menu items with no visible effect: {failures:?}"
    );
}

#[test]
fn the_file_menu_starts_a_new_worksheet() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["File", "New Worksheet"]).expect("new");
    let data = ui.app().data().expect("worksheet");
    assert_eq!(data.n_rows(), 0);
    assert!(!data.columns().is_empty());
}

#[test]
fn the_data_menu_adds_inserts_and_deletes_rows() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Add Row"]).expect("add");
    assert_eq!(ui.app().data().expect("data").n_rows(), 25);
    ui.open_menu(&["Data", "Insert Row Above"]).expect("insert");
    assert_eq!(ui.app().data().expect("data").n_rows(), 26);
    ui.open_menu(&["Data", "Delete Row"]).expect("delete");
    assert_eq!(ui.app().data().expect("data").n_rows(), 25);
}

#[test]
fn the_data_menu_adds_and_deletes_columns() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    let before = ui.app().data().expect("data").n_columns();
    for item in ["Add Numeric Column", "Add Text Column", "Add Date Column"] {
        ui.open_menu(&["Data", item]).expect("add column");
    }
    assert_eq!(ui.app().data().expect("data").n_columns(), before + 3);
    ui.open_menu(&["Data", "Delete Column"]).expect("delete");
    assert_eq!(ui.app().data().expect("data").n_columns(), before + 2);
}

#[test]
fn the_data_menu_transposes_the_worksheet() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    let (rows, columns) = shape(&ui).expect("shape");
    ui.open_menu(&["Data", "Transpose"]).expect("transpose");
    let data = ui.app().data().expect("data");
    assert_eq!(data.n_rows(), columns);
    assert!(data.n_columns() > rows);
}

#[test]
fn the_data_menu_sorts_the_rows() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Sort…"]).expect("sort");
    ui.set_text("Keys:", "weight:desc").expect("keys");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("data");
    let column = data.column("weight").expect("weight");
    let first = column.value(0).expect("first");
    let last = column.value(data.n_rows() - 1).expect("last");
    assert!(first >= last, "{first} < {last}");
}

#[test]
fn the_data_menu_stacks_and_unstacks() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Stack…"]).expect("stack");
    ui.set_text("Columns:", "weight,height").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Stacked worksheet"),
        "{}",
        ui.session()
    );
    assert_eq!(ui.app().data().expect("data").n_rows(), 48);

    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Unstack…"]).expect("unstack");
    ui.select("Value:", "weight").expect("value");
    ui.select("Group:", "batch").expect("group");
    ui.click("Run").expect("run");
    assert!(
        ui.session().contains("Unstacked worksheet"),
        "{}",
        ui.session()
    );
    let data = ui.app().data().expect("data");
    assert!(data.column("A").is_ok() && data.column("B").is_ok());
    assert_eq!(data.n_rows(), 12);
}

#[test]
fn the_data_menu_subsets_the_rows() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Subset…"]).expect("subset");
    ui.select("Column:", "batch").expect("column");
    ui.set_text("Value:", "A").expect("value");
    ui.click("Run").expect("run");
    let data = ui.app().data().expect("data");
    assert_eq!(data.n_rows(), 12);
    let column = data.column("batch").expect("batch");
    for row in 0..data.n_rows() {
        assert_eq!(column.label(row).as_deref(), Some("A"));
    }
}

#[test]
fn the_data_menu_selects_the_first_numeric_column() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.app_mut().select_cell(7, 0);
    assert_eq!(ui.app().selected_column(), Some("batch"));
    ui.open_menu(&["Data", "Select First Numeric Column"])
        .expect("select");
    assert_eq!(ui.app().selected_column(), Some("weight"));
}

#[test]
fn the_edit_menu_undoes_and_redoes() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Add Row"]).expect("add");
    assert_eq!(ui.app().data().expect("data").n_rows(), 25);
    ui.open_menu(&["Edit", "Undo"]).expect("undo");
    assert_eq!(ui.app().data().expect("data").n_rows(), 24);
    ui.open_menu(&["Edit", "Redo"]).expect("redo");
    assert_eq!(ui.app().data().expect("data").n_rows(), 25);
}

#[test]
fn the_edit_menu_clears_cells() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.app_mut().select_cell(0, 0);
    ui.open_menu(&["Edit", "Clear Cells"]).expect("clear");
    assert!(
        ui.app()
            .data()
            .expect("data")
            .column("weight")
            .expect("weight")
            .is_missing(0)
    );
}

#[test]
fn the_edit_menu_renames_a_column() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Edit", "Rename Column…"]).expect("rename");
    ui.set_text("New name:", "mass").expect("name");
    ui.click("Rename").expect("commit");
    let data = ui.app().data().expect("data");
    assert!(data.column("mass").is_ok());
    assert!(data.column("weight").is_err());
}

#[test]
fn the_edit_menu_clears_the_session() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Stat", "Basic Statistics", "Display Descriptive Statistics"])
        .expect("describe");
    assert!(!ui.session().is_empty());
    ui.open_menu(&["Edit", "Clear Session"]).expect("clear");
    assert!(ui.session().is_empty());
}

#[test]
fn the_data_dialogs_have_no_unlabeled_controls() {
    for path in [
        &["Data", "Sort…"][..],
        &["Data", "Stack…"][..],
        &["Data", "Unstack…"][..],
        &["Data", "Subset…"][..],
        &["Edit", "Rename Column…"][..],
    ] {
        let mut ui = Automation::new();
        ui.load_frame(full_frame(), "full.csv");
        ui.open_menu(path).expect("open");
        assert!(
            ui.unlabeled_widgets().is_empty(),
            "{path:?}: {:?}",
            ui.unlabeled_widgets()
        );
        ui.press_key(egui::Key::Escape);
    }
}

#[test]
fn the_tools_menu_switches_language() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    assert_eq!(ui.app().catalog().label("File"), "File");
    ui.open_menu(&["Tools", "Português"]).expect("portuguese");
    assert_eq!(ui.app().catalog().label("File"), "Ficheiro");
    ui.app_mut().set_language(fp_core::i18n::Lang::English);
    assert_eq!(ui.app().catalog().label("File"), "File");
}

#[test]
fn the_window_menu_reveals_the_plot() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Graph", "Probability Plot…"]).expect("plot");
    ui.click("OK").expect("ok");
    assert!(ui.plot().is_some());
    ui.open_menu(&["Window", "Probability Plot"])
        .expect("window");
    assert!(
        ui.window_titles()
            .iter()
            .any(|title| title == "Probability Plot"),
        "{:?}",
        ui.window_titles()
    );
}

#[test]
fn the_data_menu_changes_the_date_format() {
    let date = |text: &str| fp_core::data::parse_date(text).expect("date");
    let frame = DataFrame::new(vec![Column::new_date(
        "date",
        vec![date("2024-02-29"), date("2025-01-01")],
    )])
    .expect("date frame");
    let mut ui = Automation::new();
    ui.load_frame(frame, "dates.csv");
    ui.open_menu(&["Data", "Date Format", "DD/MM/YYYY"])
        .expect("format");
    assert_eq!(ui.app().date_format().name(), "DD/MM/YYYY");
    ui.open_menu(&["Data", "Date Format", "DD Mon YYYY"])
        .expect("format");
    assert_eq!(ui.app().date_format().name(), "DD Mon YYYY");
    ui.open_menu(&["Data", "Date Format", "ISO (YYYY-MM-DD)"])
        .expect("format");
    assert_eq!(ui.app().date_format().name(), "ISO (YYYY-MM-DD)");
}

#[test]
fn the_tools_menu_zooms_the_interface() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    assert!((ui.app().zoom() - 1.0).abs() < 1e-6);
    ui.open_menu(&["Tools", "Zoom", "150%"]).expect("zoom");
    assert!((ui.app().zoom() - 1.5).abs() < 1e-6, "{}", ui.app().zoom());
    ui.open_menu(&["Tools", "Zoom", "100%"]).expect("zoom");
    assert!((ui.app().zoom() - 1.0).abs() < 1e-6);
}

#[test]
fn the_help_menu_opens_about() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Help", "About FreePlots"]).expect("about");
    assert_eq!(ui.window_titles(), vec!["About FreePlots"]);
}

#[test]
fn the_file_menu_opens_every_open_and_save_dialog() {
    for (item, title) in [
        ("Open CSV…", "Open CSV"),
        ("Open Project…", "Open Project"),
        ("Open Excel…", "Open Excel"),
        ("Save CSV As…", "Save CSV As"),
        ("Save Project As…", "Save Project As"),
    ] {
        let mut ui = Automation::new();
        ui.load_frame(full_frame(), "full.csv");
        ui.open_menu(&["File", item]).expect("file item");
        assert_eq!(ui.window_titles(), vec![title], "{item}");
        ui.press_key(egui::Key::Escape);
    }
}

#[test]
fn the_file_menu_exports_the_session() {
    for (item, title) in [
        ("Export Session PDF…", "Export Session PDF"),
        ("Export Session HTML…", "Export Session HTML"),
    ] {
        let mut ui = Automation::new();
        ui.load_frame(full_frame(), "full.csv");
        ui.open_menu(&["Stat", "Basic Statistics", "Display Descriptive Statistics"])
            .expect("describe");
        assert!(!ui.session().is_empty());
        ui.open_menu(&["File", item]).expect("export");
        assert_eq!(ui.window_titles(), vec![title], "{item}");
        ui.press_key(egui::Key::Escape);
    }
}

#[test]
fn the_data_menu_adds_columns_with_the_right_type() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    for (item, dtype) in [
        ("Add Numeric Column", fp_core::data::DType::Float),
        ("Add Text Column", fp_core::data::DType::Text),
        ("Add Date Column", fp_core::data::DType::Date),
    ] {
        ui.open_menu(&["Data", item]).expect("add");
        let data = ui.app().data().expect("data");
        assert_eq!(data.columns().last().expect("column").dtype(), dtype);
    }
}

#[test]
fn the_data_dialogs_reject_invalid_input() {
    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Sort…"]).expect("sort");
    ui.set_text("Keys:", "weight:sideways").expect("keys");
    ui.click("Run").expect("run");
    assert!(ui.status().contains("Unknown direction"), "{}", ui.status());

    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Stack…"]).expect("stack");
    ui.set_text("Columns:", "").expect("columns");
    ui.click("Run").expect("run");
    assert!(
        ui.status().contains("at least one column"),
        "{}",
        ui.status()
    );

    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Data", "Subset…"]).expect("subset");
    ui.select("Column:", "weight").expect("column");
    ui.set_text("Value:", "not-a-number").expect("value");
    ui.click("Run").expect("run");
    assert!(!ui.status().is_empty(), "the status explains the error");

    let mut ui = Automation::new();
    ui.load_frame(full_frame(), "full.csv");
    ui.open_menu(&["Edit", "Rename Column…"]).expect("rename");
    ui.set_text("New name:", "height").expect("name");
    ui.click("Rename").expect("commit");
    assert!(!ui.status().is_empty(), "the status explains the error");
    assert!(ui.app().data().expect("data").column("weight").is_ok());
}

#[test]
fn every_menu_and_submenu_has_items() {
    let cases: &[(&[&str], &str)] = &[
        (&["File"], "New Worksheet"),
        (&["Edit"], "Undo"),
        (&["Data"], "Add Row"),
        (&["Data", "Date Format"], "ISO (YYYY-MM-DD)"),
        (&["Calc"], "Calculator…"),
        (&["Stat"], "Basic Statistics"),
        (
            &["Stat", "Basic Statistics"],
            "Display Descriptive Statistics",
        ),
        (&["Stat", "Multivariate"], "Principal Components…"),
        (&["Stat", "Time Series"], "Moving Average…"),
        (&["Stat", "Regression"], "Regression…"),
        (&["Stat", "ANOVA"], "One-Way…"),
        (&["Stat", "DOE"], "Create 2-Level Design…"),
        (&["Stat", "Reliability"], "Accelerated Life Testing…"),
        (&["Stat", "Control Charts"], "I–MR…"),
        (&["Stat", "Quality Tools"], "Capability Analysis…"),
        (&["Graph"], "Probability Plot…"),
        (&["Assistant"], "Choose an analysis…"),
        (&["Tools"], "Native plot windows"),
        (&["Tools", "Zoom"], "100%"),
        (&["Window"], "Probability Plot"),
        (&["Help"], "About FreePlots"),
        (&["Help"], "Keyboard shortcuts"),
    ];
    for (path, item) in cases {
        let mut ui = Automation::new();
        ui.load_frame(full_frame(), "full.csv");
        ui.open_menu(path)
            .unwrap_or_else(|error| panic!("open {path:?}: {error}"));
        assert!(
            ui.widgets().iter().any(|widget| {
                widget.label.as_deref().is_some_and(|label| {
                    let trimmed = label.trim_end_matches(" ▶").trim_end_matches(" ⏵");
                    label == *item || trimmed == *item
                })
            }),
            "{path:?} has no {item:?}"
        );
        ui.press_key(egui::Key::Escape);
    }
}
