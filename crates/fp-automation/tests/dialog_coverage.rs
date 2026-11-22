// SPDX-License-Identifier: MIT
//! Every dialog item of the Stat, Graph and Calc menus opens a window.

use fp_automation::Automation;
use fp_core::data::{Column, DataFrame};

/// Menu paths whose last item opens a dialog window.
const DIALOG_MENUS: &[&[&str]] = &[
    &["Calc", "Calculator…"],
    &["Calc", "Data and Calculations…"],
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
    &["Stat", "Basic Statistics", "2×2 Crossover Equivalence…"],
    &["Stat", "Basic Statistics", "Chi-Square Goodness-of-Fit…"],
    &["Stat", "Basic Statistics", "Grubbs Test…"],
    &["Stat", "Basic Statistics", "Power and Sample Size…"],
    &["Stat", "Basic Statistics", "Mann-Whitney…"],
    &["Stat", "Basic Statistics", "Wilcoxon Signed-Rank…"],
    &["Stat", "Basic Statistics", "Kruskal-Wallis…"],
    &["Stat", "Basic Statistics", "Chi-Square Test…"],
    &["Stat", "Basic Statistics", "Analysis of Means…"],
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
    &["Stat", "Multivariate", "Factor Analysis…"],
    &["Stat", "Multivariate", "Hierarchical Clustering…"],
    &["Stat", "Multivariate", "Discriminant Analysis…"],
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
    &["Stat", "Time Series", "ARIMA…"],
    &["Stat", "Time Series", "Moving Average…"],
    &["Stat", "Time Series", "Autocorrelation…"],
    &["Stat", "Time Series", "Cross Correlation…"],
    &["Stat", "Time Series", "Unit Root Test…"],
    &["Stat", "Time Series", "Differences…"],
    &["Stat", "Time Series", "Trend Analysis…"],
    &["Stat", "Time Series", "Decomposition…"],
    &["Stat", "Time Series", "Exponential Smoothing…"],
    &["Stat", "Regression", "Regression…"],
    &["Stat", "Regression", "Nonlinear Regression…"],
    &["Stat", "Regression", "CART and Random Forest…"],
    &["Stat", "Regression", "Partial Least Squares…"],
    &["Stat", "Regression", "Best Subsets…"],
    &["Stat", "Regression", "Stepwise Regression…"],
    &["Stat", "Regression", "Ordinal Logistic Regression…"],
    &["Stat", "Regression", "Poisson Regression…"],
    &["Stat", "Regression", "Binary Logistic Regression…"],
    &["Stat", "ANOVA", "General Linear Model…"],
    &["Stat", "ANOVA", "One-Way…"],
    &["Stat", "ANOVA", "MANOVA…"],
    &["Stat", "ANOVA", "Nested ANOVA…"],
    &["Stat", "ANOVA", "Levene Test…"],
    &["Stat", "ANOVA", "Bartlett Test…"],
    &["Stat", "DOE", "Create 2-Level Design…"],
    &["Stat", "DOE", "Plackett-Burman Design…"],
    &["Stat", "DOE", "Box-Behnken Design…"],
    &["Stat", "DOE", "Mixture Design…"],
    &["Stat", "DOE", "Optimal Design…"],
    &["Stat", "DOE", "General Factorial Design…"],
    &["Stat", "DOE", "2-Level Factorial…"],
    &["Stat", "DOE", "Analyze Factorial Design…"],
    &["Stat", "DOE", "Factorial Plots…"],
    &["Stat", "DOE", "Response Surface…"],
    &["Stat", "DOE", "Response Optimizer…"],
    &["Stat", "Reliability", "Accelerated Life Testing…"],
    &["Stat", "Reliability", "Step-Stress ALT…"],
    &["Stat", "Reliability", "Kaplan–Meier…"],
    &["Stat", "Reliability", "Survival Analysis…"],
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
    &["Stat", "Quality Tools", "Variables Acceptance Sampling…"],
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
    &["File", "Open Excel…"],
    &["File", "Export Session PDF…"],
    &["File", "Export Session HTML…"],
    &["Edit", "Find…"],
    &["Edit", "Format Cells…"],
    &["Edit", "Rename Column…"],
    &["Data", "Sort…"],
    &["Data", "Stack…"],
    &["Data", "Unstack…"],
    &["Data", "Subset…"],
    &["Data", "Date Difference…"],
    &["Data", "Add Days…"],
    &["Stat", "Basic Statistics", "Runs Test…"],
    &["Stat", "Basic Statistics", "1-Sample Z…"],
    &["Stat", "Basic Statistics", "1-Sample Poisson Rate…"],
    &["Stat", "Basic Statistics", "2-Sample Poisson Rate…"],
    &["Stat", "Basic Statistics", "Poisson Goodness-of-Fit…"],
    &["Stat", "Basic Statistics", "Graphical Summary…"],
    &["Stat", "Basic Statistics", "Store Descriptive Statistics…"],
    &["Stat", "Basic Statistics", "Covariance…"],
    &["Stat", "Basic Statistics", "Tally…"],
    &["Stat", "Regression", "Predict…"],
    &["Stat", "Control Charts", "CUSUM Chart…"],
    &["Stat", "Control Charts", "Acceptance Chart…"],
    &["Stat", "Control Charts", "T Chart (Rare Events)…"],
    &["Stat", "Quality Tools", "Capability Sixpack…"],
    &["Stat", "Quality Tools", "Attribute Capability…"],
    &["Stat", "Quality Tools", "Johnson Transformation…"],
    &["Graph", "Probability Distribution Plot…"],
    &["Assistant", "Test normality"],
    &["Assistant", "Control chart"],
    &["Assistant", "Process capability"],
    &["Help", "Show the tour"],
    &["Help", "Where the settings are stored…"],
    &["Help", "Diagnostics…"],
    &["Help", "Assistant for agents…"],
    &["Help", "Third-party licences…"],
    &["Help", "About FreePlots"],
];

/// Menu labels that react without opening a window.
const ACTION_MENUS: &[&[&str]] = &[
    &["File", "New Worksheet"],
    &["File", "Print…"],
    &["Edit", "Undo"],
    &["Edit", "Redo"],
    &["Edit", "Copy"],
    &["Edit", "Cut"],
    &["Edit", "Paste"],
    &["Edit", "Select All"],
    &["Edit", "Clear Cells"],
    &["Edit", "Clear Session"],
    &["Data", "Add Row"],
    &["Data", "Insert Row Above"],
    &["Data", "Delete Row"],
    &["Data", "Add Numeric Column"],
    &["Data", "Add Text Column"],
    &["Data", "Add Date Column"],
    &["Data", "Delete Column"],
    &["Data", "Transpose"],
    &["Data", "Select First Numeric Column"],
    &["Stat", "Basic Statistics", "Display Descriptive Statistics"],
    &["Stat", "Basic Statistics", "Normality Test (AD)"],
    &["Stat", "Basic Statistics", "D'Agostino–Pearson Test"],
    &["Stat", "Basic Statistics", "Kolmogorov–Smirnov Test"],
    &["Stat", "Basic Statistics", "Ryan–Joiner Test"],
    &["Stat", "Control Charts", "Run Chart"],
    &["Graph", "Boxplot"],
    &["Graph", "Histogram"],
    &["Assistant", "Describe my data"],
    &["Help", "Check for updates"],
];

/// Menu labels exercised by the interface tests in `fp-ui`.
///
/// `Exit` sends a viewport close (asserted in the File-menu test) and
/// `Show the graph` restores a closed graph (asserted in the Window-menu test);
/// neither leaves headless state this catalogue could compare.
const COVERED_ELSEWHERE: &[&str] = &["Exit", "Show the graph"];

/// Headings that are not actions.
const SECTION_LABELS: &[&str] = &["Language"];

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
    // A report enables the Session exports and gives the catalogue a session
    // to compare when a menu item only appends text.
    ui.open_menu(&["Stat", "Basic Statistics", "Display Descriptive Statistics"])
        .expect("seed the session");
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

/// Everything the menus can change without opening a window.
#[derive(Debug, PartialEq)]
struct Fingerprint {
    status: String,
    session: String,
    windows: Vec<String>,
    selected: Option<String>,
    shape: Option<(usize, usize)>,
    sheets: usize,
    active_sheet: usize,
    selection: Option<(usize, usize, usize, usize)>,
    cells: Vec<Option<String>>,
    clipboard: String,
    dark: bool,
    language: String,
    plot: Option<String>,
    print: Option<std::path::PathBuf>,
    url: Option<String>,
}

fn fingerprint(ui: &mut Automation) -> Fingerprint {
    let shape = ui.app().data().map(|df| (df.n_rows(), df.n_columns()));
    let cells = ui
        .app()
        .data()
        .map(|df| {
            (0..df.n_columns())
                .flat_map(|column| (0..df.n_rows()).map(move |row| (column, row)))
                .map(|(column, row)| df.cell_text(column, row))
                .collect()
        })
        .unwrap_or_default();
    Fingerprint {
        status: ui.status().to_owned(),
        session: ui.session().to_owned(),
        windows: ui.window_titles(),
        selected: ui.app().selected_column().map(str::to_owned),
        shape,
        sheets: ui.app().worksheet_count(),
        active_sheet: ui.app().active_sheet(),
        selection: ui.app().selection_bounds(),
        cells,
        clipboard: ui.app().clipboard_text().to_owned(),
        dark: ui.app().dark_mode(),
        language: format!("{:?}", ui.app().language()),
        plot: ui.plot().map(|plot| plot.title.clone()),
        print: ui.app_mut().take_print_request(),
        url: ui.app_mut().take_open_url_request(),
    }
}

/// Gives the action the premise it needs to do anything (history, clipboard,
/// a report to clear, a text column to move away from).
fn prepare_action(ui: &mut Automation, path: &[&str]) {
    match *path {
        ["Edit", "Undo"] => {
            ui.app_mut().select_cell(0, 0);
            ui.app_mut().set_edit_buffer("99");
            assert!(ui.app_mut().commit_cell(), "the edit for {path:?}");
            ui.app_mut().select_cell(0, 0);
        }
        ["Edit", "Redo"] => {
            ui.app_mut().select_cell(0, 0);
            ui.app_mut().set_edit_buffer("99");
            assert!(ui.app_mut().commit_cell(), "the edit for {path:?}");
            ui.app_mut().undo();
            ui.app_mut().select_cell(0, 0);
        }
        ["Edit", "Paste"] => {
            ui.open_menu(&["Edit", "Copy"]).expect("copy before paste");
        }
        ["Edit", "Clear Session"] => {
            ui.open_menu(&["Stat", "Basic Statistics", "Display Descriptive Statistics"])
                .expect("a report to clear");
        }
        ["Data", "Select First Numeric Column"] => {
            let text_column = ui
                .app()
                .data()
                .and_then(|df| df.column_index("batch").ok())
                .unwrap_or(0);
            ui.app_mut().select_cell(text_column, 0);
        }
        _ => {}
    }
}

fn read_nothing(_path: &std::path::Path) -> Option<String> {
    None
}

fn write_nothing(_path: &std::path::Path, _text: &str) -> Result<(), String> {
    Ok(())
}

fn create_nothing(_path: &std::path::Path) -> Result<(), String> {
    Ok(())
}

fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir()
}

#[test]
fn every_action_menu_changes_the_application() {
    let mut failures = Vec::new();
    for path in ACTION_MENUS {
        let mut ui = Automation::new();
        ui.app_mut().set_storage(fp_ui::Storage::new(
            read_nothing,
            write_nothing,
            create_nothing,
            temp_dir,
        ));
        ui.load_frame(frame(), "actions.csv");
        prepare_action(&mut ui, path);
        let before = fingerprint(&mut ui);
        ui.open_menu(path)
            .unwrap_or_else(|error| panic!("open {path:?}: {error}"));
        let after = fingerprint(&mut ui);
        if before == after {
            failures.push(path.join(" > "));
        }
    }
    assert!(
        failures.is_empty(),
        "menu actions with no effect: {failures:?}"
    );
}

/// Every `.label("…")` in the menu bar, in source order.
fn menu_labels() -> Vec<String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fp-ui/src/app/menu_bar.rs");
    let text = std::fs::read_to_string(&path).expect("menu_bar.rs");
    let mut labels: Vec<String> = Vec::new();
    let mut rest = text.as_str();
    while let Some(index) = rest.find(".label(\"") {
        let after = &rest[index + ".label(\"".len()..];
        if let Some(end) = after.find('"') {
            let label = &after[..end];
            if !labels.iter().any(|existing| existing == label) {
                labels.push(label.to_owned());
            }
        }
        rest = after;
    }
    labels
}

/// The labels passed to `help_menu_button` (menus and submenus).
fn menu_headers() -> Vec<String> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fp-ui/src/app/menu_bar.rs");
    let text = std::fs::read_to_string(&path).expect("menu_bar.rs");
    let marker = "help_menu_button(ui, self.catalog().label(\"";
    let mut labels: Vec<String> = Vec::new();
    let mut rest = text.as_str();
    while let Some(index) = rest.find(marker) {
        let after = &rest[index + marker.len()..];
        if let Some(end) = after.find('"') {
            let label = &after[..end];
            if !labels.iter().any(|existing| existing == label) {
                labels.push(label.to_owned());
            }
        }
        rest = after;
    }
    labels
}

#[test]
fn every_menu_label_is_in_the_catalogue() {
    let labels = menu_labels();
    let headers = menu_headers();

    let mut covered: Vec<&str> = Vec::new();
    for path in DIALOG_MENUS.iter().chain(ACTION_MENUS.iter()) {
        covered.push(path.last().copied().expect("a non-empty path"));
    }
    covered.extend(COVERED_ELSEWHERE);
    covered.extend(SECTION_LABELS);

    let missing: Vec<&String> = labels
        .iter()
        .filter(|label| !headers.contains(label) && !covered.contains(&label.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "menu labels with no catalogue entry: {missing:#?}"
    );

    // A few items are written as plain strings in the menu bar instead of
    // going through the catalogue, so check the source text for those too.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fp-ui/src/app/menu_bar.rs"),
    )
    .expect("menu_bar.rs");
    let unknown: Vec<String> = DIALOG_MENUS
        .iter()
        .chain(ACTION_MENUS.iter())
        .filter(|path| {
            let last = path.last().copied().expect("a non-empty path");
            !source.contains(&format!("\"{last}\""))
        })
        .map(|path| path.join(" > "))
        .collect();
    assert!(
        unknown.is_empty(),
        "catalogue paths that are not in the menu bar: {unknown:#?}"
    );
}
