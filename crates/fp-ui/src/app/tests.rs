// SPDX-License-Identifier: MIT
use super::*;
use fp_core::data::Column;

/// Three-step test with twelve units, all failing at the last step.
fn step_stress_frame() -> DataFrame {
    let mut duration = Vec::new();
    let mut stress = Vec::new();
    let mut unit = Vec::new();
    let mut event = Vec::new();
    let levels = [100.0, 120.0, 140.0];
    for index in 0..12 {
        let first = 40.0 + f64::from(index % 4);
        let second = 20.0 + 0.5 * f64::from(index % 3);
        let third = 10.0 + 0.25 * f64::from(index % 2);
        for (step, (level, time)) in levels.iter().zip([first, second, third]).enumerate() {
            duration.push(time);
            stress.push(*level);
            unit.push(format!("unit {}", index + 1));
            event.push(f64::from(u8::from(step == 2)));
        }
    }
    DataFrame::new(vec![
        Column::new_f64("duration", duration),
        Column::new_f64("stress", stress),
        Column::new_text("unit", unit),
        Column::new_f64("event", event),
    ])
    .expect("frame")
}

/// Two numeric columns of ten observations, for two-sample and paired tests.
fn two_sample_frame() -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64(
            "before",
            vec![12.1, 11.8, 12.4, 11.9, 12.3, 12.0, 11.7, 12.2, 12.5, 11.6],
        ),
        Column::new_f64(
            "after",
            vec![12.6, 12.4, 12.9, 12.3, 12.8, 12.5, 12.2, 12.7, 12.9, 12.1],
        ),
    ])
    .expect("frame")
}

/// Forty observations for the time-series dialogs.
fn series_values_frame() -> DataFrame {
    let values: Vec<f64> = (0..40)
        .map(|index| 10.0 + 0.2 * f64::from(index) + 2.0 * f64::from(index % 4))
        .collect();
    DataFrame::new(vec![Column::new_f64("y", values)]).expect("frame")
}

/// Gage R&R data: ten parts, two operators, two replicates.
fn gage_frame() -> DataFrame {
    let mut measure = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    for p in 1..=10 {
        let reference = 10.0 + f64::from(p);
        for op in ["A", "B"] {
            for rep in 0..2 {
                measure
                    .push(reference + 0.05 * f64::from(rep) + if op == "A" { 0.1 } else { -0.1 });
                part.push(format!("P{p}"));
                operator.push(op.to_owned());
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measure", measure),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("frame")
}

/// Attribute agreement data: five parts, three appraisers, one rating each.
fn attribute_frame() -> DataFrame {
    let mut part = Vec::new();
    let mut appraiser = Vec::new();
    let mut rating = Vec::new();
    for p in 1..=5 {
        for (index, who) in ["A", "B", "C"].iter().enumerate() {
            part.push(format!("P{p}"));
            appraiser.push((*who).to_owned());
            rating.push(if f64::from(p as u8) + index as f64 > 4.0 {
                "pass".to_owned()
            } else {
                "fail".to_owned()
            });
        }
    }
    DataFrame::new(vec![
        Column::new_text("part", part),
        Column::new_text("appraiser", appraiser),
        Column::new_text("rating", rating),
    ])
    .expect("frame")
}

/// Linearity data: five reference levels with three replicates each.
fn linearity_frame() -> DataFrame {
    let mut measure = Vec::new();
    let mut reference = Vec::new();
    for level in 1..=5 {
        for rep in 0..3 {
            let base = f64::from(level) * 2.0;
            measure.push(base + 0.1 * f64::from(rep) + 0.05);
            reference.push(base);
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measure", measure),
        Column::new_f64("reference", reference),
    ])
    .expect("frame")
}

/// 2x2 factorial with two replicates, a block column and a response.
fn factorial_frame() -> DataFrame {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    let mut block = Vec::new();
    for run in 0..2 {
        for (level_a, level_b) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            a.push(level_a);
            b.push(level_b);
            y.push(10.0 + 2.0 * level_a - level_b + 0.1 * f64::from(run));
            block.push("1".to_owned());
        }
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
        Column::new_text("block", block),
    ])
    .expect("frame")
}

/// L8 Taguchi design with a response.
fn taguchi_frame() -> DataFrame {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    let mut y = Vec::new();
    for run in 0..8u8 {
        let level = |bit: u8| if run & bit == 0 { 1.0 } else { 2.0 };
        let (la, lb, lc) = (level(1), level(2), level(4));
        a.push(la);
        b.push(lb);
        c.push(lc);
        y.push(20.0 + 1.5 * (la - 1.5) + 0.8 * (lb - 1.5) - 0.6 * (lc - 1.5));
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("C", c),
        Column::new_f64("y", y),
    ])
    .expect("frame")
}

/// Life data: ten times, two censored, two stress levels and two modes.
fn life_frame() -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64(
            "time",
            vec![10.0, 12.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0],
        ),
        Column::new_f64(
            "censor",
            vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0],
        ),
        Column::new_f64(
            "stress",
            vec![
                100.0, 100.0, 100.0, 100.0, 100.0, 120.0, 120.0, 120.0, 120.0, 120.0,
            ],
        ),
        Column::new_text(
            "mode",
            vec![
                "A".to_owned(),
                "B".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
            ],
        ),
    ])
    .expect("frame")
}

/// Twenty observations with two predictors: y depends on x1 and x2.
fn regression_frame() -> DataFrame {
    let mut x1 = Vec::new();
    let mut x2 = Vec::new();
    let mut x3 = Vec::new();
    let mut y = Vec::new();
    for index in 0..20 {
        let a = f64::from(index);
        let b = f64::from(index % 5);
        let c = f64::from(index % 3);
        x1.push(a);
        x2.push(b);
        x3.push(c);
        y.push(5.0 + 2.0 * a + 0.5 * b + 0.05 * c);
    }
    DataFrame::new(vec![
        Column::new_f64("x1", x1),
        Column::new_f64("x2", x2),
        Column::new_f64("x3", x3),
        Column::new_f64("y", y),
    ])
    .expect("frame")
}

/// Balanced two-factor design: 3 groups x 2 levels x 4 replicates.
fn two_way_frame() -> DataFrame {
    let mut y = Vec::new();
    let mut g = Vec::new();
    let mut h = Vec::new();
    for (group, level) in [
        ("A", "low"),
        ("B", "low"),
        ("C", "low"),
        ("A", "high"),
        ("B", "high"),
        ("C", "high"),
    ] {
        for replicate in 0..4 {
            let base = match group {
                "A" => 10.0,
                "B" => 12.0,
                _ => 14.0,
            };
            let lift = if level == "high" { 1.0 } else { 0.0 };
            y.push(base + lift + 0.2 * f64::from(replicate));
            g.push(group.to_owned());
            h.push(level.to_owned());
        }
    }
    DataFrame::new(vec![
        Column::new_f64("y", y),
        Column::new_text("g", g),
        Column::new_text("h", h),
    ])
    .expect("frame")
}

/// Thirty observations with a small drift, for the control charts.
fn process_frame() -> DataFrame {
    let values: Vec<f64> = (0..30)
        .map(|index| 10.0 + 0.05 * f64::from(index % 7) + 0.02 * f64::from(index))
        .collect();
    DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
}

/// Two 0/1 columns for the proportion tests.
fn proportion_frame() -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64(
            "pass",
            vec![
                1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0,
                1.0, 1.0, 1.0, 0.0,
            ],
        ),
        Column::new_f64(
            "retest",
            vec![
                1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0,
                1.0, 0.0, 1.0, 0.0,
            ],
        ),
    ])
    .expect("frame")
}

/// Full 3x3 factorial with a quadratic response and three replicates.
fn desirability_frame() -> DataFrame {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for replicate in 0..3 {
        for i in 0..3 {
            for j in 0..3 {
                let x = f64::from(i) - 1.0;
                let z = f64::from(j) - 1.0;
                a.push(x);
                b.push(z);
                y.push(
                    10.0 + 2.0 * x + 3.0 * z - 1.5 * x * x - 2.0 * z * z
                        + x * z
                        + 0.1 * f64::from(replicate),
                );
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("a", a),
        Column::new_f64("b", b),
        Column::new_f64("y", y),
    ])
    .expect("frame")
}

fn frame() -> DataFrame {
    DataFrame::new(vec![
        Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
        Column::new_text("batch", vec!["A".into(); 5]),
    ])
    .expect("frame")
}

#[test]
fn starts_with_an_empty_grid() {
    let app = FreePlotsApp::new();
    let data = app.data().expect("the grid is visible at startup");
    assert_eq!(data.n_columns(), 8);
    assert_eq!(data.n_rows(), 0);
    assert_eq!(app.selected_column(), Some("C1"));
    assert_eq!(app.selected_cell(), Some((0, 0)));
    assert_eq!(app.worksheet_count(), 1);
    assert_eq!(app.sheet_names(), vec!["Worksheet 1".to_owned()]);
    assert!(app.session_text().is_empty());
    assert!(app.plot().is_none());
}

#[test]
fn typing_over_a_cell_starts_and_commits_the_edit() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("7".to_owned()));
    harness.run();
    assert_eq!(harness.state().inline_edit(), Some((0, 0)));
    assert_eq!(harness.state().inline_buffer(), "7");
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    harness.run();
    assert_eq!(harness.state().inline_edit(), None);
    assert_eq!(
        harness.state().data().unwrap().cell_text(0, 0).as_deref(),
        Some("7")
    );
}

#[test]
fn the_tab_arrows_scroll_the_worksheet_strip() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.add_sheet();
    app.add_sheet();
    app.add_sheet();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert_eq!(harness.state().worksheet_count(), 4);
    harness.get_by_label("▶").click();
    harness.run();
    assert_eq!(harness.state().tab_first(), 1);
    harness.get_by_label("▶|").click();
    harness.run();
    assert_eq!(harness.state().tab_first(), 3);
    harness.get_by_label("|◀").click();
    harness.run();
    assert_eq!(harness.state().tab_first(), 0);
}

#[test]
fn a_paste_event_fills_a_block() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    // egui-winit delivers Ctrl+V as a Paste event carrying the clipboard
    // text; it never reports the V key itself.
    harness.event(egui::Event::Paste("7\t8\n9\t10\n".to_owned()));
    harness.run();
    let data = harness.state().data().expect("frame");
    assert_eq!(data.cell_text(0, 0).as_deref(), Some("7"));
    assert_eq!(data.cell_text(1, 0).as_deref(), Some("8"));
    assert_eq!(data.cell_text(0, 1).as_deref(), Some("9"));
    assert_eq!(data.cell_text(1, 1).as_deref(), Some("10"));
}

#[test]
fn a_click_anywhere_in_an_empty_cell_selects_it() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    // The right-hand half of an empty C2 cell: there is no text there.
    let pos = egui::pos2(170.0, 205.0);
    harness.hover_at(pos);
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    let selected = harness.state().selected_cell();
    assert_eq!(selected, Some((1, 2)));
}

#[test]
fn the_global_shortcuts_reach_the_worksheet() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    harness.run();
    assert_eq!(harness.state().selection_bounds(), Some((0, 0, 1, 4)));
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
    harness.run();
    harness.run();
    assert_eq!(harness.state().worksheet_count(), 2);
}

#[test]
fn the_help_menu_opens_the_shortcuts() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Keyboard shortcuts").click();
    harness.run();
    assert!(harness.state().show_shortcuts);
    assert!(
        harness
            .get_by_label("Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z")
            .rect()
            .width()
            > 0.0
    );
}

#[test]
fn the_dark_mode_is_opt_in_and_applies_the_dark_theme() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert!(!harness.state().dark_mode);
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Dark mode").click();
    harness.run();
    harness.run();
    assert!(harness.state().dark_mode);
    assert!(!harness.state().light_visuals);
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Dark mode").click();
    harness.run();
    harness.run();
    assert!(!harness.state().dark_mode);
    assert!(harness.state().light_visuals);
}

#[test]
fn the_assistant_guides_to_a_dialog() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Assistant").click();
    harness.run();
    harness.get_by_label("Choose an analysis…").click();
    harness.run();
    assert!(harness.state().show_assistant_dialog);
    harness.get_by_label("Compare two groups").click();
    harness.run();
    assert!(!harness.state().show_assistant_dialog);
    assert!(harness.state().show_t_dialog);
}

#[test]
fn the_file_menu_requests_a_print() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("Print…").click();
    harness.run();
    assert!(harness.state_mut().take_print_request().is_some());
}

#[test]
fn the_about_formula_spells_freeplots() {
    let mut letters: Vec<char> = [ABOUT_NUMERATOR, ABOUT_DENOMINATOR, ABOUT_FACTOR]
        .iter()
        .flat_map(|part| part.chars())
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    letters.sort_unstable();
    assert_eq!(letters, vec!['E', 'E', 'F', 'L', 'O', 'P', 'R', 'S', 'T']);
}

#[test]
fn the_about_dialog_has_no_clear_button() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.show_about = true;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    // The Session panel still has its own Clear; the About window has none.
    let window = harness.get_by_label("About FreePlots").rect();
    assert!(
        !harness
            .get_all_by_label("Clear")
            .any(|button| window.contains(button.rect().center())),
        "the About dialog must not show a Clear button"
    );
    assert!(harness.get_by_label("MIT license").rect().width() > 0.0);
    assert!(
        harness
            .get_by_label("Visit the public FreePlots repository")
            .rect()
            .width()
            > 0.0
    );
    // The formula header is left-aligned, not centred in the dialog.
    let formula = harness.get_by_label("FreePlots =").rect();
    assert!(
        formula.left() - window.left() < 40.0,
        "the About header is not left-aligned"
    );
    assert!(
        harness
            .get_by_label("FreePlots Engine 0.1.0")
            .rect()
            .width()
            > 0.0
    );
}

#[test]
fn the_about_link_asks_the_runner_to_open_the_browser() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.show_about = true;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness
        .get_by_label("Visit the public FreePlots repository")
        .click();
    harness.run();
    assert_eq!(
        harness.state_mut().take_open_url_request().as_deref(),
        Some("https://github.com/nasp2000/FreePlots")
    );
}

#[test]
fn an_open_dialog_raises_the_window() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.show_save = true;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert!(harness.state().dialogs_on_top);
    harness.state_mut().show_save = false;
    harness.run();
    assert!(!harness.state().dialogs_on_top);
}

#[test]
fn the_diagnostics_dialog_copies_the_report_and_opens_the_folder() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_diagnostics(
        "FreePlots 0.1.0\nOS: test\nData folder: /tmp\nLog: /tmp/freeplots.log".to_owned(),
        std::path::PathBuf::from("/tmp/freeplots.log"),
    );
    app.open_diagnostics();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Diagnostics");
    harness.get_by_label("Copy diagnostics").click();
    harness.run();
    assert!(harness.state_mut().take_copy_diagnostics_request());
    harness.get_by_label("Open data folder").click();
    harness.run();
    assert!(harness.state_mut().take_open_data_dir_request());
    assert!(!harness.state_mut().take_open_data_dir_request());
}

#[test]
fn the_zoom_limit_keeps_the_minimum_layout_visible() {
    use super::chrome::maximum_zoom;

    assert!((maximum_zoom(egui::vec2(1024.0, 760.0)) - 1024.0 / 760.0).abs() < 1e-6);
    assert!((maximum_zoom(egui::vec2(1920.0, 1080.0)) - 1080.0 / 520.0).abs() < 1e-6);
    assert!((maximum_zoom(egui::vec2(3840.0, 2160.0)) - 2.5).abs() < 1e-6);
    assert!((maximum_zoom(egui::vec2(800.0, 600.0)) - 800.0 / 760.0).abs() < 1e-6);
}

#[test]
fn a_small_window_clamps_the_zoom() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.zoom = 2.0;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1024.0, 760.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    let expected = 1024.0 / 760.0;
    assert!(
        (harness.state().zoom - expected).abs() < 1e-3,
        "zoom {}",
        harness.state().zoom
    );
}

#[test]
fn the_plus_button_adds_a_worksheet_tab() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert_eq!(harness.state().worksheet_count(), 1);
    {
        // The status bar also has a zoom "+"; the tab button sits above it.
        let buttons: Vec<_> = harness.get_all_by_label("+").collect();
        let tab = buttons
            .into_iter()
            .min_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
            .expect("tab +");
        tab.click();
    }
    harness.run();
    assert_eq!(harness.state().worksheet_count(), 2);
    assert_eq!(harness.state().active_sheet(), 1);
    assert_eq!(harness.state().sheet_names()[1], "Worksheet 2");
    assert_eq!(harness.state().data().map(DataFrame::n_rows), Some(0));
    harness.state_mut().switch_sheet(0);
    assert_eq!(harness.state().data().map(DataFrame::n_rows), Some(5));
    assert_eq!(harness.state().source(), "weights.csv");
}

#[test]
fn closing_a_worksheet_keeps_the_last_one() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_sheet();
    assert_eq!(app.worksheet_count(), 2);
    assert!(app.close_sheet(1));
    assert_eq!(app.worksheet_count(), 1);
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    assert!(!app.close_sheet(0));
    assert!(app.status().contains("at least one"));
    assert_eq!(app.worksheet_count(), 1);
}

#[test]
fn closing_the_active_worksheet_loads_another() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_sheet();
    app.select_cell(0, 0);
    app.set_edit_buffer("7");
    assert!(app.commit_cell());
    assert_eq!(app.active_sheet(), 1);
    assert!(app.close_sheet(1));
    assert_eq!(app.worksheet_count(), 1);
    assert_eq!(app.active_sheet(), 0);
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
}

#[test]
fn a_worksheet_can_be_renamed() {
    let mut app = FreePlotsApp::new();
    app.add_sheet();
    app.open_sheet_rename(1);
    assert!(app.show_sheet_rename);
    app.sheet_rename_text = "  Sales  ".to_owned();
    app.commit_sheet_rename();
    assert_eq!(app.sheet_names()[1], "Sales");
    assert!(!app.show_sheet_rename);

    app.open_sheet_rename(1);
    app.sheet_rename_text = "   ".to_owned();
    app.commit_sheet_rename();
    assert_eq!(app.sheet_names()[1], "Sales");
    assert!(app.status().contains("cannot be empty"));
}

#[test]
fn the_headers_select_rows_columns_and_the_whole_sheet() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_column(1);
    assert_eq!(app.selection_bounds(), Some((1, 0, 1, 4)));
    assert_eq!(app.selection_size(), (1, 5));
    app.select_row(2);
    assert_eq!(app.selection_bounds(), Some((0, 2, 1, 2)));
    assert_eq!(app.selection_size(), (2, 1));
    app.select_all_cells();
    assert_eq!(app.selection_bounds(), Some((0, 0, 1, 4)));
    assert_eq!(app.selection_size(), (2, 5));
    // The whole column copies as one tab-separated line per row.
    app.select_column(0);
    assert_eq!(
        app.selection_tsv().as_deref(),
        Some("12.5\n13.1\n11.9\n12.8\n12.2")
    );
}

#[test]
fn switching_worksheets_keeps_each_sheet_data_and_undo() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_sheet();
    app.select_cell(0, 0);
    app.set_edit_buffer("42");
    assert!(app.commit_cell());

    app.switch_sheet(0);
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    app.switch_sheet(1);
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("42"));

    // The undo history travels with its own worksheet.
    app.undo();
    assert_ne!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("42"));
    app.switch_sheet(0);
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
}

#[test]
fn opening_a_file_keeps_a_non_empty_worksheet() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    assert_eq!(app.worksheet_count(), 1);
    app.set_data(frame(), "other.csv".to_owned());
    assert_eq!(app.worksheet_count(), 2);
    assert_eq!(app.active_sheet(), 1);
    assert_eq!(app.source(), "other.csv");
    app.switch_sheet(0);
    assert_eq!(app.source(), "weights.csv");
}

#[test]
fn loading_selects_first_numeric_column() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    assert_eq!(app.selected_column(), Some("weight"));
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    assert!(app.status().contains("5 rows"));
}

#[test]
fn analysis_fills_session_and_plot() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_probability_plot();
    assert!(app.session_text().contains("Probability Plot"));
    assert!(app.session_text().contains("Anderson–Darling"));
    let plot = app.plot().expect("plot");
    assert_eq!(plot.x_axis.label, "weight");
    assert_eq!(plot.y_axis.label, "Percent");
    assert_eq!(plot.points.len(), 5);
}

#[test]
fn lognormal_analysis_uses_a_log_axis() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "life",
        vec![1.0, 2.0, 4.0, 8.0, 16.0],
    )])
    .expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.distribution = DistributionKind::Lognormal;
    app.run_probability_plot();
    assert!(app.session_text().contains("Lognormal"));
    assert!(matches!(
        app.plot().expect("plot").x_axis.scale,
        fp_core::plot::Scale::Log10
    ));
}

#[test]
fn weibull_analysis_uses_the_gumbel_axis() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "life",
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
    )])
    .expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.distribution = DistributionKind::Weibull;
    app.run_probability_plot();
    assert!(app.session_text().contains("Weibull"));
    assert!(matches!(
        app.plot().expect("plot").y_axis.scale,
        fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Gumbel)
    ));
}

#[test]
fn logistic_analysis_uses_the_logit_axis() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "score",
        vec![-2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
    )])
    .expect("frame");
    app.set_data(df, "score.csv".to_owned());
    app.distribution = DistributionKind::Logistic;
    app.run_probability_plot();
    assert!(app.session_text().contains("Logistic"));
    assert!(matches!(
        app.plot().expect("plot").y_axis.scale,
        fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Logit)
    ));
}

#[test]
fn loglogistic_analysis_uses_a_log_axis() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "life",
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
    )])
    .expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.distribution = DistributionKind::Loglogistic;
    app.run_probability_plot();
    assert!(app.session_text().contains("Loglogistic"));
    assert!(matches!(
        app.plot().expect("plot").x_axis.scale,
        fp_core::plot::Scale::Log10
    ));
}

#[test]
fn gamma_analysis_uses_the_normal_probability_axis() {
    use fp_core::dists::{Distribution, Gamma};

    let mut app = FreePlotsApp::new();
    let dist = Gamma::new(2.0, 5.0).expect("gamma");
    let values: Vec<f64> = (1..=40)
        .map(|i| {
            let p = (i as f64 - 0.5) / 40.0;
            dist.quantile(p)
        })
        .collect();
    let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.distribution = DistributionKind::Gamma;
    app.run_probability_plot();
    assert!(app.session_text().contains("Gamma"));
    assert!(matches!(
        app.plot().expect("plot").y_axis.scale,
        fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Normal)
    ));
}

#[test]
fn extreme_value_analysis_uses_the_extreme_value_axis() {
    use fp_core::dists::{Distribution, ExtremeValue};

    let mut app = FreePlotsApp::new();
    let dist = ExtremeValue::new(2.0, 1.5).expect("extreme value");
    let values: Vec<f64> = (1..=40)
        .map(|i| {
            let p = (i as f64 - 0.5) / 40.0;
            dist.quantile(p)
        })
        .collect();
    let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.distribution = DistributionKind::ExtremeValue;
    app.run_probability_plot();
    assert!(app.session_text().contains("Extreme Value"));
    assert!(matches!(
        app.plot().expect("plot").y_axis.scale,
        fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::ExtremeValue)
    ));
}

#[test]
fn display_dialog_can_be_opened_and_painted() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.open_probability_plot_dialog();
    assert!(app.show_probability_plot_dialog);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    let painted = output.shapes.len();
    output.textures_delta.clear();
    assert!(painted > 0);
}

#[test]
fn multi_y_report_and_overlay_reach_the_ui() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
        Column::new_f64("height", vec![21.5, 22.1, 20.9, 21.8, 21.2]),
    ])
    .expect("frame");
    app.set_data(df, "two.csv".to_owned());
    app.execute_spec(
        AnalysisSpec::probability_plot_multi(vec!["weight".to_owned(), "height".to_owned()])
            .with_layout(PlotLayout::Overlay),
        "multi".to_owned(),
    );
    assert!(app.session_text().contains("column weight"));
    assert!(app.session_text().contains("column height"));
    assert_eq!(app.plot().expect("plot").series.len(), 2);
}

#[test]
fn grouping_reaches_the_ui_as_two_series() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![11.0, 11.5, 12.0, 13.0, 13.5, 14.0]),
        Column::new_text(
            "batch",
            vec![
                "A".into(),
                "A".into(),
                "A".into(),
                "B".into(),
                "B".into(),
                "B".into(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "grouped.csv".to_owned());
    // Grouping follows the layout switch: the overlay is asked for here,
    // the separate panels are the default.
    app.execute_spec(
        AnalysisSpec::probability_plot("weight")
            .with_group("batch")
            .with_layout(fp_core::workflow::PlotLayout::Overlay),
        "grouped".to_owned(),
    );
    assert_eq!(app.plot().expect("plot").series.len(), 2);
    assert!(app.session_text().contains("weight (A)"));
    assert!(app.session_text().contains("weight (B)"));
}

#[test]
fn symbols_display_drops_line_and_band() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let spec = AnalysisSpec::probability_plot("weight").with_display(PlotDisplay::Symbols);
    app.execute_spec(spec, "test".to_owned());
    let plot = app.plot().expect("plot");
    assert!(!plot.points.is_empty());
    assert!(plot.lines.is_empty());
    assert!(plot.bands.is_empty());
}

#[test]
fn capability_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.execute_spec(
        AnalysisSpec::capability("weight", Some(8.0), Some(12.0)),
        "capability".to_owned(),
    );
    assert!(app.session_text().contains("Capability Analysis"));
    assert!(app.session_text().contains("Cpk"));
    assert!(app.session_text().contains("PPM"));
    app.execute_spec(
        AnalysisSpec::capability("weight", Some(8.0), Some(12.0)).with_capability_confidence(0.90),
        "capability 90".to_owned(),
    );
    assert!(app.session_text().contains("Cpk 90% CI"));
    app.execute_spec(
        AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_distribution(DistributionKind::Weibull),
        "capability Weibull".to_owned(),
    );
    assert!(app.session_text().contains("Capability Analysis: Weibull"));
    assert!(app.session_text().contains("PPM total"));
}

#[test]
fn t_test_dialog_runs_one_and_two_sample_tests() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("before", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
    ])
    .expect("frame");
    app.set_data(df, "pairs.csv".to_owned());
    app.open_t_test_dialog(TTestKind::OneSample);
    app.t_null = "3".to_owned();
    app.run_t_test_dialog();
    assert!(app.session_text().contains("1-Sample t"));
    assert!(app.session_text().contains("p-value"));

    app.open_t_test_dialog(TTestKind::TwoSample);
    app.run_t_test_dialog();
    assert!(app.session_text().contains("2-Sample t"));
    assert!(app.session_text().contains("Welch"));

    app.open_t_test_dialog(TTestKind::Paired);
    app.run_t_test_dialog();
    assert!(app.session_text().contains("Paired t"));
}

#[test]
fn the_new_dialogs_can_be_opened_and_painted() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0]),
        Column::new_text(
            "g",
            vec![
                "x".to_owned(),
                "x".to_owned(),
                "x".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "weights.csv".to_owned());

    let ctx = egui::Context::default();
    let paint = |app: &mut FreePlotsApp| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
    };

    for kind in [
        DiagnosticKind::ChiSquareGoodnessOfFit,
        DiagnosticKind::Grubbs,
    ] {
        app.open_diagnostic_dialog(kind);
        paint(&mut app);
    }
    for kind in [
        RankTestKind::MannWhitney,
        RankTestKind::Wilcoxon,
        RankTestKind::KruskalWallis,
        RankTestKind::Friedman,
        RankTestKind::MoodsMedian,
    ] {
        app.open_rank_test_dialog(kind);
        paint(&mut app);
    }
    for kind in [
        EquivalenceKind::OneSample,
        EquivalenceKind::TwoSample,
        EquivalenceKind::Paired,
    ] {
        app.open_equivalence_dialog(kind);
        paint(&mut app);
    }
    app.open_power_dialog();
    paint(&mut app);
    app.open_tolerance_dialog();
    paint(&mut app);
    app.open_box_behnken_dialog();
    paint(&mut app);
    app.open_linearity_dialog();
    paint(&mut app);
    app.open_logistic_dialog();
    paint(&mut app);
    app.open_timeseries_dialog();
    paint(&mut app);
    app.open_pca_dialog();
    paint(&mut app);
    app.open_kmeans_dialog();
    paint(&mut app);
    app.open_agreement_dialog();
    paint(&mut app);
    app.open_sort_dialog();
    paint(&mut app);
    app.open_stack_dialog();
    paint(&mut app);
    app.open_unstack_dialog();
    paint(&mut app);
    app.open_subset_dialog();
    paint(&mut app);
    app.open_pb_dialog();
    paint(&mut app);
    app.open_poisson_dialog();
    paint(&mut app);
    app.open_ordinal_dialog();
    paint(&mut app);
    app.open_pareto_dialog();
    paint(&mut app);
    app.open_calc_dialog();
    paint(&mut app);
    app.open_alt_dialog();
    paint(&mut app);
    app.open_t2_dialog();
    paint(&mut app);
    app.open_sampling_dialog();
    paint(&mut app);
    app.open_taguchi_dialog();
    paint(&mut app);
    app.open_taguchi_design_dialog();
    paint(&mut app);
    for kind in [
        TTestKind::OneSample,
        TTestKind::TwoSample,
        TTestKind::Paired,
    ] {
        app.open_t_test_dialog(kind);
        paint(&mut app);
    }
    for kind in [
        Test2Kind::OneProportion,
        Test2Kind::TwoProportions,
        Test2Kind::OneVariance,
        Test2Kind::TwoVariances,
    ] {
        app.open_test2_dialog(kind);
        paint(&mut app);
    }
    for kind in [OneWayKind::Levene, OneWayKind::Bartlett] {
        app.open_one_way_dialog(kind);
        paint(&mut app);
    }
    for opener in [
        FreePlotsApp::open_anova_dialog,
        FreePlotsApp::open_attribute_dialog,
        FreePlotsApp::open_box_cox_dialog,
        FreePlotsApp::open_correlation_dialog,
        FreePlotsApp::open_crosstab_dialog,
        FreePlotsApp::open_curve_dialog,
        FreePlotsApp::open_design_dialog,
        FreePlotsApp::open_doe_dialog,
        FreePlotsApp::open_dotplot_dialog,
        FreePlotsApp::open_ecdf_dialog,
        FreePlotsApp::open_exponential_dialog,
        FreePlotsApp::open_gage_dialog,
        FreePlotsApp::open_g_chart_dialog,
        FreePlotsApp::open_interval_dialog,
        FreePlotsApp::open_kaplan_meier_dialog,
        FreePlotsApp::open_lognormal_dialog,
        FreePlotsApp::open_matrix_dialog,
        FreePlotsApp::open_partial_dialog,
        FreePlotsApp::open_probability_plot_dialog,
        FreePlotsApp::open_regression_dialog,
        FreePlotsApp::open_rsm_dialog,
        FreePlotsApp::open_scatter_dialog,
        FreePlotsApp::open_series_dialog,
        FreePlotsApp::open_stem_dialog,
        FreePlotsApp::open_stepwise_dialog,
        FreePlotsApp::open_two_way_dialog,
        FreePlotsApp::open_type1_dialog,
        FreePlotsApp::open_weibull_dialog,
    ] {
        opener(&mut app);
        paint(&mut app);
    }
}

#[test]
fn power_dialog_covers_the_anova_and_regression_procedures() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0])]).expect("frame");
    app.set_data(df, "values.csv".to_owned());

    app.open_power_dialog();
    app.power_procedure = PowerProcedure::Anova;
    app.power_effect = "1".to_owned();
    app.power_groups = 3;
    app.power_n = "10".to_owned();
    app.run_power_dialog();
    assert!(app.session_text().contains("One-Way ANOVA"));

    app.open_power_dialog();
    app.power_procedure = PowerProcedure::Regression;
    app.power_effect = "0.3".to_owned();
    app.power_groups = 2;
    app.power_n = "40".to_owned();
    app.run_power_dialog();
    assert!(app.session_text().contains("Regression"));
}

#[test]
fn power_dialog_reports_the_power_and_the_sample_size() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0])]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_power_dialog();
    app.power_effect = "0.5".to_owned();
    app.power_n = "64".to_owned();
    app.run_power_dialog();
    let text = app.session_text();
    assert!(text.contains("Power and Sample Size"), "{text}");
    assert!(text.contains("Power"), "{text}");

    app.open_power_dialog();
    app.power_effect = "1.0".to_owned();
    app.power_n = "0".to_owned();
    app.power_target = 80.0;
    app.run_power_dialog();
    let text = app.session_text();
    assert!(text.contains("Sample size"), "{text}");
}

#[test]
fn diagnostic_dialog_runs_the_goodness_of_fit_and_outlier_tests() {
    let mut app = FreePlotsApp::new();
    let counts =
        DataFrame::new(vec![Column::new_f64("count", vec![20.0, 30.0, 50.0])]).expect("frame");
    app.set_data(counts, "counts.csv".to_owned());
    app.open_diagnostic_dialog(DiagnosticKind::ChiSquareGoodnessOfFit);
    app.diagnostic_column = "count".to_owned();
    app.diagnostic_expected = "25,25,50".to_owned();
    app.run_diagnostic_dialog();
    assert!(app.session_text().contains("Chi-Square Goodness-of-Fit"));

    let weights = DataFrame::new(vec![Column::new_f64(
        "weight",
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0],
    )])
    .expect("frame");
    app.set_data(weights, "weights.csv".to_owned());
    app.open_diagnostic_dialog(DiagnosticKind::Grubbs);
    app.diagnostic_column = "weight".to_owned();
    app.run_diagnostic_dialog();
    assert!(app.session_text().contains("Grubbs Test"));
}

#[test]
fn one_way_dialog_runs_the_variance_tests() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![1.0, 2.0, 3.0, 4.0, 20.0, 25.0]),
        Column::new_text(
            "g",
            vec![
                "x".to_owned(),
                "x".to_owned(),
                "x".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "weights.csv".to_owned());

    app.open_one_way_dialog(OneWayKind::Levene);
    app.anova_response = "weight".to_owned();
    app.anova_factor = "g".to_owned();
    app.run_anova_dialog();
    assert!(app.session_text().contains("Levene Test"));

    app.open_one_way_dialog(OneWayKind::Bartlett);
    app.anova_response = "weight".to_owned();
    app.anova_factor = "g".to_owned();
    app.run_anova_dialog();
    assert!(app.session_text().contains("Bartlett Test"));
}

#[test]
fn alt_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let times = vec![
        10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 30.0, 34.0, 38.0, 42.0, 46.0, 50.0,
    ];
    let stress = vec![
        400.0, 400.0, 400.0, 400.0, 400.0, 400.0, 350.0, 350.0, 350.0, 350.0, 350.0, 350.0,
    ];
    let df = DataFrame::new(vec![
        Column::new_f64("time", times),
        Column::new_f64("stress", stress),
    ])
    .expect("frame");
    app.set_data(df, "life.csv".to_owned());
    app.open_alt_dialog();
    app.alt_time = "time".to_owned();
    app.alt_stress = "stress".to_owned();
    app.alt_use_stress = 300.0;
    app.run_alt_dialog();
    let text = app.session_text();
    assert!(text.contains("Accelerated Life"), "{text}");
    assert!(app.plot().is_some());

    app.open_alt_dialog();
    app.alt_stress = "time".to_owned();
    app.run_alt_dialog();
    assert!(app.status.contains("different"), "{}", app.status);
}

#[test]
fn calc_dialog_adds_a_column() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![1.0, 4.0, 9.0]),
        Column::new_f64("offset", vec![1.0, 2.0, 3.0]),
    ])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_calc_dialog();
    app.calc_name = "root".to_owned();
    app.calc_expression = "abs(weight) + offset * 2".to_owned();
    app.run_calc_dialog();
    let data = app.data().expect("worksheet");
    assert!(data.column("root").is_ok());
    assert_eq!(data.n_rows(), 3);
    assert!(app.session_text().contains("Column root"));

    // A bad expression reports the error and leaves the sheet alone.
    app.open_calc_dialog();
    app.calc_name = "broken".to_owned();
    app.calc_expression = "1 / (weight - 1)".to_owned();
    app.run_calc_dialog();
    assert!(app.status.contains("row 1"), "{}", app.status);
    assert!(app.data().expect("worksheet").column("broken").is_err());
}

#[test]
fn sign_test_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "x",
        vec![8.0, 9.0, 11.0, 12.0, 13.0, 14.0, 15.0],
    )])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_rank_test_dialog(RankTestKind::SignTest);
    app.rank_first = "x".to_owned();
    app.rank_median = "10".to_owned();
    app.run_rank_test_dialog();
    let text = app.session_text();
    assert!(text.contains("Sign Test"), "{text}");
    assert!(text.contains("Hypothesised median"), "{text}");

    // An empty field tests against 0, as the documented behaviour does.
    app.session.clear();
    app.open_rank_test_dialog(RankTestKind::SignTest);
    app.rank_first = "x".to_owned();
    app.rank_median.clear();
    app.run_rank_test_dialog();
    assert!(
        app.session_text().contains("median 0"),
        "{}",
        app.session_text()
    );
}

#[test]
fn pareto_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_text(
        "defect",
        vec![
            "scratch".to_owned(),
            "scratch".to_owned(),
            "scratch".to_owned(),
            "dent".to_owned(),
            "dent".to_owned(),
            "crack".to_owned(),
        ],
    )])
    .expect("frame");
    app.set_data(df, "defects.csv".to_owned());
    app.open_pareto_dialog();
    app.pareto_column = "defect".to_owned();
    app.run_pareto_dialog();
    let text = app.session_text();
    assert!(text.contains("Pareto Chart"), "{text}");
    assert!(text.contains("Vital few"), "{text}");
    assert!(app.plot().is_some());
}

#[test]
fn the_pareto_dialog_carries_a_weight_column() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_text(
            "defect",
            vec![
                "scratch".to_owned(),
                "scratch".to_owned(),
                "dent".to_owned(),
                "crack".to_owned(),
            ],
        ),
        Column::new_f64("cost", vec![1.0, 1.0, 8.0, 2.0]),
    ])
    .expect("frame");
    app.set_data(df, "defects.csv".to_owned());
    app.open_pareto_dialog();
    assert!(app.pareto_weight.is_empty(), "a new dialog has no weight");
    app.pareto_column = "defect".to_owned();
    app.pareto_weight = "cost".to_owned();
    app.run_pareto_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let text = app.session_text();
    assert!(text.contains("weighted by cost"), "{text}");
    assert!(text.contains("Weight"), "{text}");
    assert!(text.contains("dent"), "{text}");
    // Opening the dialog again clears the weight, so a later run is the plain
    // count unless the user chooses one.
    app.open_pareto_dialog();
    assert!(app.pareto_weight.is_empty());
}

#[test]
fn ordinal_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    // Ordered codes 1..3 rising with the predictor but overlapping, so the
    // likelihood has a finite maximum (perfect separation has none).
    let y = vec![1.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 3.0, 2.0, 3.0, 3.0, 3.0];
    let x = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1];
    let df = DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
    app.set_data(df, "ordinal.csv".to_owned());
    app.open_ordinal_dialog();
    app.ordinal_response = "y".to_owned();
    app.ordinal_predictors = "x".to_owned();
    app.run_ordinal_dialog();
    let text = app.session_text();
    assert!(text.contains("Ordinal Logistic Regression"), "{text}");
    assert!(text.contains("Thresholds"), "{text}");

    app.open_ordinal_dialog();
    app.ordinal_predictors.clear();
    app.run_ordinal_dialog();
    assert!(app.status.contains("predictor"), "{}", app.status);
}

#[test]
fn taguchi_dialogs_reach_the_session() {
    let mut app = FreePlotsApp::new();
    app.open_taguchi_design_dialog();
    app.taguchi_runs = 8;
    app.taguchi_design_factors = 3;
    app.design_randomize = false;
    app.run_taguchi_design_dialog();
    let df = app.data().expect("worksheet");
    assert_eq!(df.n_rows(), 8);
    assert_eq!(df.n_columns(), 5);
    assert!(app.session_text().contains("Taguchi design generated"));

    app.open_taguchi_dialog();
    app.taguchi_response = "A".to_owned();
    app.taguchi_factors = "B,C".to_owned();
    app.taguchi_quality = fp_core::taguchi::Quality::Smaller;
    app.run_taguchi_dialog();
    let text = app.session_text();
    assert!(text.contains("Taguchi Analysis"), "{text}");
    assert!(text.contains("Best level"), "{text}");

    // The response cannot also be a factor.
    app.open_taguchi_dialog();
    app.taguchi_response = "A".to_owned();
    app.taguchi_factors = "A,B".to_owned();
    app.run_taguchi_dialog();
    assert!(app.status.contains("factor"), "{}", app.status);
}

#[test]
fn sampling_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    app.open_sampling_dialog();
    app.sampling_plan_mode = false;
    app.sampling_sample = 20;
    app.sampling_accept = 1;
    app.sampling_lot = 0;
    app.run_sampling_dialog();
    let text = app.session_text();
    assert!(text.contains("Acceptance Sampling"), "{text}");
    assert!(text.contains("OC Curve"), "{text}");
    assert!(app.plot().is_some());

    // Planning mode needs no worksheet.
    app.open_sampling_dialog();
    app.sampling_plan_mode = true;
    app.sampling_aql = 1.0;
    app.sampling_ltpd = 5.0;
    app.sampling_alpha = 5.0;
    app.sampling_beta = 10.0;
    app.run_sampling_dialog();
    assert!(
        app.session_text().contains("Producer risk"),
        "{}",
        app.session_text()
    );
}

#[test]
fn t2_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    // Ten subgroups of three observations, written out literally.
    let offsets = [(0.05, -0.1), (-0.02, 0.08), (0.03, 0.01)];
    let mut a = Vec::new();
    let mut b = Vec::new();
    for _ in 0..10 {
        for (offset_a, offset_b) in offsets {
            a.push(10.0 + offset_a);
            b.push(20.0 + offset_b);
        }
    }
    let df = DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame");
    app.set_data(df, "t2.csv".to_owned());
    app.open_t2_dialog();
    app.t2_columns = "a,b".to_owned();
    app.t2_subgroup = 3;
    app.run_t2_dialog();
    let text = app.session_text();
    assert!(text.contains("Hotelling T"), "{text}");
    assert!(text.contains("UCL"), "{text}");
    assert!(app.plot().is_some());

    app.open_t2_dialog();
    app.t2_columns = "a".to_owned();
    app.run_t2_dialog();
    assert!(app.status.contains("two columns"), "{}", app.status);
}

#[test]
fn poisson_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    // Growing counts against a growing predictor, written out so that the
    // UI layer does no arithmetic of its own.
    let y = vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 5.0, 6.0, 7.0];
    let x = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
    let df = DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
    app.set_data(df, "counts.csv".to_owned());
    app.open_poisson_dialog();
    app.poisson_response = "y".to_owned();
    app.poisson_predictors = "x".to_owned();
    app.run_poisson_dialog();
    let text = app.session_text();
    assert!(text.contains("Poisson Regression"), "{text}");
    assert!(text.contains("Rate ratio"), "{text}");

    app.open_poisson_dialog();
    app.poisson_predictors.clear();
    app.run_poisson_dialog();
    assert!(app.status.contains("predictor"), "{}", app.status);
}

#[test]
fn plackett_burman_dialog_writes_the_design() {
    let mut app = FreePlotsApp::new();
    app.open_pb_dialog();
    app.design_runs = 12;
    app.design_randomize = false;
    app.run_pb_dialog();
    let df = app.data().expect("worksheet");
    assert_eq!(df.n_rows(), 12);
    assert_eq!(df.n_columns(), 13);
    assert!(app.session_text().contains("Plackett-Burman"));

    // An unavailable run count reaches the status bar.
    app.open_pb_dialog();
    app.design_runs = 10;
    app.run_pb_dialog();
    assert!(app.status.contains("Plackett"), "{}", app.status);
}

#[test]
fn data_menus_transform_the_worksheet() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("y", vec![3.0, 1.0, 2.0, 4.0]),
        Column::new_text(
            "g",
            vec![
                "b".to_owned(),
                "a".to_owned(),
                "b".to_owned(),
                "a".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());

    // Sort by the numeric column, descending.
    app.open_sort_dialog();
    app.sort_keys = "y:desc".to_owned();
    app.run_sort_dialog();
    assert_eq!(
        app.data().expect("data").cell_text(0, 0).as_deref(),
        Some("4")
    );

    // Subset on the text column.
    app.open_subset_dialog();
    app.subset_column = "g".to_owned();
    app.subset_op = fp_core::reshape::SubsetOp::Equal;
    app.subset_value = "a".to_owned();
    app.run_subset_dialog();
    assert_eq!(app.data().expect("data").n_rows(), 2);

    // Stack, unstack and transpose round-trip through dialogs.
    app.open_stack_dialog();
    app.stack_columns = "y".to_owned();
    app.run_stack_dialog();
    assert!(app.data().expect("data").column("Variable").is_ok());

    app.open_unstack_dialog();
    app.unstack_value = "Value".to_owned();
    app.unstack_group = "Variable".to_owned();
    app.run_unstack_dialog();
    assert!(app.data().expect("data").column("y").is_ok());

    app.run_transpose();
    assert!(app.data().expect("data").n_columns() > 1);

    // A condition that matches nothing reports the error.
    app.open_subset_dialog();
    app.subset_column = "Column".to_owned();
    app.subset_op = fp_core::reshape::SubsetOp::Equal;
    app.subset_value = "does-not-exist".to_owned();
    app.run_subset_dialog();
    assert!(
        app.status.contains("empty") || app.status.contains("Empty"),
        "{}",
        app.status
    );
}

#[test]
fn agreement_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut part = Vec::new();
    let mut appraiser = Vec::new();
    let mut rating = Vec::new();
    for part_label in ["1", "2", "3"] {
        let verdict = if part_label == "2" { "fail" } else { "pass" };
        for who in ["A", "B"] {
            for _ in 0..2 {
                part.push(part_label.to_owned());
                appraiser.push(who.to_owned());
                rating.push(verdict.to_owned());
            }
        }
    }
    let df = DataFrame::new(vec![
        Column::new_text("part", part),
        Column::new_text("appraiser", appraiser),
        Column::new_text("rating", rating),
    ])
    .expect("frame");
    app.set_data(df, "agreement.csv".to_owned());
    app.open_agreement_dialog();
    app.agreement_part = "part".to_owned();
    app.agreement_appraiser = "appraiser".to_owned();
    app.agreement_rating = "rating".to_owned();
    app.run_agreement_dialog();
    let text = app.session_text();
    assert!(text.contains("Attribute Agreement"), "{text}");
    assert!(text.contains("Kappa"), "{text}");

    app.open_agreement_dialog();
    app.agreement_rating = "part".to_owned();
    app.run_agreement_dialog();
    assert!(app.status.contains("different"), "{}", app.status);
}

#[test]
fn kmeans_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut x = Vec::new();
    let mut y = Vec::new();
    for centre in [(0.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
        for step in 0..6 {
            x.push(centre.0 + step as f64 * 0.1);
            y.push(centre.1 + step as f64 * 0.1);
        }
    }
    let df = DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
    app.set_data(df, "clusters.csv".to_owned());
    app.open_kmeans_dialog();
    app.kmeans_columns = "x,y".to_owned();
    app.kmeans_clusters = 3;
    app.run_kmeans_dialog();
    let text = app.session_text();
    assert!(text.contains("K-Means Clustering"), "{text}");
    assert!(text.contains("Cluster"), "{text}");
    assert!(app.plot().is_some());

    app.open_kmeans_dialog();
    app.kmeans_columns = "x".to_owned();
    app.run_kmeans_dialog();
    assert!(app.status.contains("two columns"), "{}", app.status);
}

#[test]
fn pca_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    for index in 0..30 {
        let x = index as f64;
        a.push(x);
        b.push(2.0 * x + ((index % 3) as f64) * 0.01);
        c.push(((index * 7) % 11) as f64);
    }
    let df = DataFrame::new(vec![
        Column::new_f64("a", a),
        Column::new_f64("b", b),
        Column::new_f64("c", c),
    ])
    .expect("frame");
    app.set_data(df, "pca.csv".to_owned());
    app.open_pca_dialog();
    app.pca_columns = "a,b,c".to_owned();
    app.pca_keep = 2;
    app.run_pca_dialog();
    let text = app.session_text();
    assert!(text.contains("Principal Component Analysis"), "{text}");
    assert!(text.contains("Eigenvalue"), "{text}");
    assert!(app.plot().is_some());

    app.open_pca_dialog();
    app.pca_columns = "a".to_owned();
    app.run_pca_dialog();
    assert!(app.status.contains("two columns"), "{}", app.status);
}

#[test]
fn timeseries_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut values = Vec::new();
    for index in 0..36 {
        values.push(100.0 + 0.8 * index as f64 + 8.0 * ((index % 12) as f64).sin());
    }
    let df = DataFrame::new(vec![Column::new_f64("sales", values)]).expect("frame");
    app.set_data(df, "sales.csv".to_owned());

    app.open_timeseries_dialog();
    app.timeseries_column = "sales".to_owned();
    app.timeseries_kind = TimeSeriesKind::MovingAverage;
    app.timeseries_window = 4;
    app.run_timeseries_dialog();
    assert!(
        app.session_text().contains("Moving average"),
        "{}",
        app.session_text()
    );
    assert!(app.plot().is_some());

    app.open_timeseries_dialog();
    app.timeseries_column = "sales".to_owned();
    app.timeseries_kind = TimeSeriesKind::Decomposition;
    app.timeseries_period = 12;
    app.run_timeseries_dialog();
    assert!(
        app.session_text().contains("decomposition"),
        "{}",
        app.session_text()
    );

    app.open_timeseries_dialog();
    app.timeseries_column = "sales".to_owned();
    app.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
    app.timeseries_season = 12;
    app.timeseries_horizon = 3;
    app.run_timeseries_dialog();
    assert!(
        app.session_text().contains("Holt-Winters"),
        "{}",
        app.session_text()
    );
}

#[test]
fn logistic_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut response = Vec::new();
    let mut x = Vec::new();
    for index in 0..40 {
        x.push(index as f64);
        let event = (index >= 20 && index % 4 != 0) || (index < 5 && index % 2 == 0);
        response.push(if event { 1.0 } else { 0.0 });
    }
    let df = DataFrame::new(vec![
        Column::new_f64("y", response),
        Column::new_f64("x", x),
    ])
    .expect("frame");
    app.set_data(df, "logit.csv".to_owned());
    app.open_logistic_dialog();
    app.logistic_response = "y".to_owned();
    app.logistic_predictors = "x".to_owned();
    app.run_logistic_dialog();
    let text = app.session_text();
    assert!(text.contains("Binary Logistic Regression"), "{text}");
    assert!(text.contains("Odds ratio"), "{text}");

    // An empty predictor list is refused.
    app.open_logistic_dialog();
    app.logistic_predictors.clear();
    app.run_logistic_dialog();
    assert!(app.status.contains("predictor"), "{}", app.status);
}

#[test]
fn logistic_dialog_accepts_a_categorical_predictor() {
    let mut app = FreePlotsApp::new();
    let mut group = Vec::new();
    let mut response = Vec::new();
    for (level, events) in [("A", 2), ("B", 5), ("C", 8)] {
        for index in 0..10 {
            group.push(level.to_owned());
            response.push(if index < events { 1.0 } else { 0.0 });
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("y", response),
        Column::new_text("group", group),
    ])
    .expect("frame");
    app.set_data(df, "logit.csv".to_owned());
    app.open_logistic_dialog();
    app.logistic_response = "y".to_owned();
    app.logistic_predictors = "group".to_owned();
    app.run_logistic_dialog();
    let text = app.session_text();
    assert!(text.contains("group[B]"), "{text}");
    assert!(text.contains("group[C]"), "{text}");
    assert!(text.contains("dummy coded"), "{text}");
}

#[test]
fn linearity_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut measurement = Vec::new();
    let mut reference = Vec::new();
    for target in [10.0, 20.0, 30.0, 40.0] {
        for offset in [-0.2, 0.0, 0.2] {
            measurement.push(target + 0.1 * target - 0.1 + offset);
            reference.push(target);
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("measurement", measurement),
        Column::new_f64("reference", reference),
    ])
    .expect("frame");
    app.set_data(df, "linearity.csv".to_owned());
    app.open_linearity_dialog();
    app.linearity_measurement = "measurement".to_owned();
    app.linearity_reference = "reference".to_owned();
    app.run_linearity_dialog();
    let text = app.session_text();
    assert!(text.contains("Gage Linearity"), "{text}");
    assert!(text.contains("%Linearity"), "{text}");
    assert!(app.plot().is_some());

    // The same column twice is refused before the analysis runs.
    app.open_linearity_dialog();
    app.linearity_reference = "measurement".to_owned();
    app.run_linearity_dialog();
    assert!(app.status.contains("different columns"), "{}", app.status);
}

#[test]
fn box_behnken_dialog_writes_the_design() {
    let mut app = FreePlotsApp::new();
    app.open_box_behnken_dialog();
    app.design_factors = "A,B,C".to_owned();
    app.design_centers = 3;
    app.design_randomize = false;
    app.run_box_behnken_dialog();
    let df = app.data().expect("worksheet");
    assert_eq!(df.n_rows(), 15);
    assert!(df.column("StdOrder").is_ok());
    assert!(app.session_text().contains("Box-Behnken design generated"));

    // Two factors are not enough and the error reaches the status bar.
    app.open_box_behnken_dialog();
    app.design_factors = "A,B".to_owned();
    app.run_box_behnken_dialog();
    assert!(app.status.contains("Box-Behnken"), "{}", app.status);
}

#[test]
fn tolerance_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=30).map(|i| 10.0 + (i % 7) as f64 * 0.3).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_tolerance_dialog();
    app.tolerance_column = "x".to_owned();
    app.tolerance_proportion = 99.0;
    app.tolerance_confidence = 95.0;
    app.run_tolerance_dialog();
    let text = app.session_text();
    assert!(text.contains("Tolerance Interval"), "{text}");
    assert!(text.contains("Lower limit"), "{text}");
    assert!(text.contains("99.0%"), "{text}");

    // Without a column the dialog reports the error.
    app.open_tolerance_dialog();
    app.tolerance_column.clear();
    app.run_tolerance_dialog();
    assert_eq!(app.status, "Select the column to analyse");
}

#[test]
fn box_cox_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = vec![
        1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0, 144.0, 233.0,
    ];
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_box_cox_dialog();
    app.box_cox_column = "x".to_owned();
    app.run_box_cox_dialog();
    let text = app.session_text();
    assert!(text.contains("Box-Cox"), "{text}");
    assert!(text.contains("p-value after"), "{text}");
}

#[test]
fn partial_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
        Column::new_f64("y", vec![2.0, 4.0, 5.0, 8.0, 9.0, 11.0, 12.0]),
        Column::new_f64("z", vec![1.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
    ])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_partial_dialog();
    app.partial_x = "x".to_owned();
    app.partial_y = "y".to_owned();
    app.partial_control = "z".to_owned();
    app.run_partial_dialog();
    assert!(app.session_text().contains("Partial Correlation"));
}

#[test]
fn g_chart_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "events",
        vec![1.0, 2.0, 1.0, 3.0, 2.0, 1.0],
    )])
    .expect("frame");
    app.set_data(df, "events.csv".to_owned());
    app.open_g_chart_dialog();
    app.g_chart_counts = "events".to_owned();
    app.run_g_chart_dialog();
    let text = app.session_text();
    assert!(text.contains("G Chart"), "{text}");
    assert!(text.contains("Center line"), "{text}");
}

#[test]
fn crosstab_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_text(
            "r",
            vec![
                "a".to_owned(),
                "a".to_owned(),
                "a".to_owned(),
                "b".to_owned(),
                "b".to_owned(),
                "b".to_owned(),
            ],
        ),
        Column::new_text(
            "c",
            vec![
                "x".to_owned(),
                "x".to_owned(),
                "x".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "table.csv".to_owned());
    app.open_crosstab_dialog();
    app.crosstab_row = "r".to_owned();
    app.crosstab_column = "c".to_owned();
    app.run_crosstab_dialog();
    let text = app.session_text();
    assert!(text.contains("Chi-Square Test for Independence"), "{text}");
    assert!(text.contains("Cramer's V"), "{text}");
}

#[test]
fn two_way_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("y", vec![10.0, 11.0, 13.0, 14.0, 10.1, 11.1, 13.1, 14.1]),
        Column::new_text(
            "a",
            vec![
                "a1".to_owned(),
                "a1".to_owned(),
                "a2".to_owned(),
                "a2".to_owned(),
                "a1".to_owned(),
                "a1".to_owned(),
                "a2".to_owned(),
                "a2".to_owned(),
            ],
        ),
        Column::new_text(
            "b",
            vec![
                "b1".to_owned(),
                "b2".to_owned(),
                "b1".to_owned(),
                "b2".to_owned(),
                "b1".to_owned(),
                "b2".to_owned(),
                "b1".to_owned(),
                "b2".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "design.csv".to_owned());
    app.open_two_way_dialog();
    app.two_way_response = "y".to_owned();
    app.two_way_first = "a".to_owned();
    app.two_way_second = "b".to_owned();
    app.run_two_way_dialog();
    assert!(app.session_text().contains("Two-Way ANOVA"));
}

#[test]
fn curve_dialog_fits_the_model() {
    let mut app = FreePlotsApp::new();
    let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.5).collect();
    let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
    app.set_data(df, "curve.csv".to_owned());
    app.open_curve_dialog();
    app.curve_x = "x".to_owned();
    app.curve_y = "y".to_owned();
    app.curve_model = CurveModel::Polynomial(1);
    app.run_curve_dialog();
    let text = app.session_text();
    assert!(text.contains("Curve Fit"), "{text}");
    assert!(text.contains("R²"), "{text}");
    assert!(app.plot().is_some());
}

#[test]
fn correlation_dialog_reports_the_pairs() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        Column::new_f64("b", vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0]),
    ])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_correlation_dialog();
    app.correlation_columns = "a,b".to_owned();
    app.run_correlation_dialog();
    let text = app.session_text();
    assert!(text.contains("Correlation"), "{text}");
    assert!(text.contains("a × b"), "{text}");
}

#[test]
fn the_language_switch_translates_the_session_and_the_menus() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=20).map(|i| i as f64 * 1.5).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    assert_eq!(app.language(), fp_core::i18n::Lang::English);

    app.set_language(fp_core::i18n::Lang::Portuguese);
    assert_eq!(app.language(), fp_core::i18n::Lang::Portuguese);
    assert_eq!(app.catalog().label("File"), "Ficheiro");

    app.run_dagostino_test();
    let text = app.session_text();
    assert!(text.contains("Estatísticas"), "{text}");
}

#[test]
fn the_dagostino_test_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=20).map(|i| i as f64 * 1.5).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.run_dagostino_test();
    assert!(app.session_text().contains("D'Agostino"));
}

#[test]
fn rank_test_dialog_runs_the_non_parametric_tests() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("b", vec![10.0, 11.0, 12.0, 13.0, 14.0]),
        Column::new_text(
            "g",
            vec![
                "x".to_owned(),
                "x".to_owned(),
                "y".to_owned(),
                "y".to_owned(),
                "x".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "pairs.csv".to_owned());

    app.open_rank_test_dialog(RankTestKind::MannWhitney);
    app.rank_second = "b".to_owned();
    app.run_rank_test_dialog();
    assert!(app.session_text().contains("Mann-Whitney Test"));

    app.open_rank_test_dialog(RankTestKind::Wilcoxon);
    app.rank_second = "b".to_owned();
    app.run_rank_test_dialog();
    assert!(app.session_text().contains("Wilcoxon Signed-Rank Test"));

    app.open_rank_test_dialog(RankTestKind::KruskalWallis);
    app.rank_first = "a".to_owned();
    app.rank_factor = "g".to_owned();
    app.run_rank_test_dialog();
    assert!(app.session_text().contains("Kruskal-Wallis Test"));

    app.open_rank_test_dialog(RankTestKind::Friedman);
    app.rank_first = "a".to_owned();
    app.rank_extra_columns = "b".to_owned();
    app.run_rank_test_dialog();
    assert!(app.session_text().contains("Friedman Test"));

    app.open_rank_test_dialog(RankTestKind::MoodsMedian);
    app.rank_first = "a".to_owned();
    app.rank_factor = "g".to_owned();
    app.run_rank_test_dialog();
    assert!(app.session_text().contains("Mood's Median Test"));
}

#[test]
fn equivalence_dialog_runs_the_to_st_tests() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("before", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
    ])
    .expect("frame");
    app.set_data(df, "pairs.csv".to_owned());

    app.open_equivalence_dialog(EquivalenceKind::OneSample);
    app.equivalence_null = "3".to_owned();
    app.equivalence_margin = "1".to_owned();
    app.run_equivalence_dialog();
    assert!(app.session_text().contains("Equivalence Test: 1-Sample"));

    app.open_equivalence_dialog(EquivalenceKind::TwoSample);
    app.equivalence_second = "after".to_owned();
    app.equivalence_margin = "1".to_owned();
    app.run_equivalence_dialog();
    assert!(app.session_text().contains("Equivalence Test: 2-Sample"));

    app.open_equivalence_dialog(EquivalenceKind::Paired);
    app.equivalence_second = "after".to_owned();
    app.equivalence_margin = "1".to_owned();
    app.run_equivalence_dialog();
    assert!(app.session_text().contains("Equivalence Test: Paired"));
}

#[test]
fn lognormal_reliability_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0]),
        Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
    ])
    .expect("frame");
    app.set_data(df, "lognormal.csv".to_owned());
    app.open_lognormal_dialog();
    app.log_time = "time".to_owned();
    app.log_censor = Some("status".to_owned());
    app.log_censor_value = "0".to_owned();
    app.run_lognormal_dialog();
    assert!(app.session_text().contains("Lognormal Reliability"));
    assert!(app.session_text().contains("B10 life"));
    assert!(!app.plot().expect("plot").lines.is_empty());
}

#[test]
fn exponential_reliability_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 10.0]),
        Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0]),
    ])
    .expect("frame");
    app.set_data(df, "exponential.csv".to_owned());
    app.open_exponential_dialog();
    app.exp_time = "time".to_owned();
    app.exp_censor = Some("status".to_owned());
    app.exp_censor_value = "0".to_owned();
    app.run_exponential_dialog();
    assert!(app.session_text().contains("Exponential Reliability"));
    assert!(app.session_text().contains("B10 life"));
    assert!(!app.plot().expect("plot").lines.is_empty());
}

#[test]
fn weibull_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]),
        Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
    ])
    .expect("frame");
    app.set_data(df, "weibull.csv".to_owned());
    app.open_weibull_dialog();
    app.weibull_time = "time".to_owned();
    app.weibull_censor = Some("status".to_owned());
    app.weibull_censor_value = "0".to_owned();
    app.run_weibull_dialog();
    assert!(app.session_text().contains("Weibull Reliability"));
    assert!(app.session_text().contains("B10 life"));
    assert!(!app.plot().expect("plot").lines.is_empty());
}

#[test]
fn boxplot_and_scatter_reach_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
    ])
    .expect("frame");
    app.set_data(df, "graphs.csv".to_owned());
    app.run_boxplot("y");
    assert!(app.session_text().contains("Boxplot"));
    app.run_histogram("y");
    assert!(app.session_text().contains("Histogram"));
    app.open_scatter_dialog();
    app.scatter_x = "x".to_owned();
    app.scatter_y = "y".to_owned();
    app.run_scatter_dialog();
    assert!(app.session_text().contains("Scatterplot"));
    assert!(app.session_text().contains("Pearson r"));
    assert_eq!(app.plot().expect("plot").points.len(), 5);
}

#[test]
fn design_dialog_generates_a_factorial_worksheet() {
    let mut app = FreePlotsApp::new();
    app.open_design_dialog();
    app.design_factors = "A,B".to_owned();
    app.design_replicates = 2;
    app.design_centers = 1;
    app.design_randomize = false;
    app.run_design_dialog();
    let df = app.data().expect("worksheet");
    assert_eq!(df.n_rows(), 9);
    assert_eq!(df.n_columns(), 4);
    assert!(app.session_text().contains("DOE design generated"));
}

#[test]
fn rsm_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let mut rows = Vec::new();
    for a in [-1.0, 0.0, 1.0] {
        for b in [-1.0, 0.0, 1.0] {
            rows.push((
                a,
                b,
                10.0 + 2.0 * a + 3.0 * b + a * b + 0.5 * a * a + 0.25 * b * b,
            ));
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
        Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
        Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
    ])
    .expect("frame");
    app.set_data(df, "rsm.csv".to_owned());
    app.open_rsm_dialog();
    app.rsm_response = "y".to_owned();
    app.rsm_factors = vec!["A".to_owned(), "B".to_owned()];
    app.rsm_contours = true;
    app.run_rsm_dialog();
    assert!(app.session_text().contains("Response Surface"));
    assert!(app.session_text().contains("A^2"));
    assert_eq!(app.plot().expect("plot").panels.len(), 3);
}

#[test]
fn blocked_factorial_uses_the_block_column() {
    let mut app = FreePlotsApp::new();
    let mut rows = Vec::new();
    for block in 0..2 {
        for a in [-1.0, 1.0] {
            for b in [-1.0, 1.0] {
                for _ in 0..2 {
                    let offset = if block == 1 { 5.0 } else { 0.0 };
                    rows.push((a, b, 10.0 + 2.0 * a + 3.0 * b + offset, block));
                }
            }
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
        Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
        Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        Column::new_text(
            "Block",
            rows.iter().map(|row| (row.3 + 1).to_string()).collect(),
        ),
    ])
    .expect("frame");
    app.set_data(df, "blocked.csv".to_owned());
    app.open_doe_dialog();
    app.doe_response = "y".to_owned();
    app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
    app.doe_block = "Block".to_owned();
    app.run_doe_dialog();
    assert!(
        app.session_text().contains("Blocks"),
        "{}",
        app.session_text()
    );
    assert!(app.session_text().contains("Factorial model"));
}

#[test]
fn a_registered_plugin_runs_from_the_workspace() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("x", vec![1.0, 2.0, f64::NAN]),
        Column::new_text(
            "label",
            vec!["a".to_owned(), "a".to_owned(), "b".to_owned()],
        ),
    ])
    .expect("frame");
    app.set_data(df, "plugins.csv".to_owned());
    app.run_plugin("data-quality");
    assert!(
        app.session_text().contains("Data Quality"),
        "{}",
        app.session_text()
    );
    assert!(app.session_text().contains("Missing values"));
}

#[test]
fn the_browse_button_emits_a_native_dialog_request() {
    let mut app = FreePlotsApp::new();
    app.begin_open(FileKind::Csv);
    // The dialog is drawn by the runner; the request is what the UI owns.
    app.request_browse(FileKind::Csv, false);
    let request = app.take_browse_request().expect("browse request");
    assert!(!request.save);
    assert!(
        request
            .filters
            .iter()
            .any(|filter| filter.contains("*.csv"))
    );
    app.set_browsed_path("C:\\data\\file.csv".to_owned(), false);
    assert_eq!(app.open_path(), "C:\\data\\file.csv");
}

#[test]
fn factorial_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let mut rows = Vec::new();
    for a in [-1.0, 1.0] {
        for b in [-1.0, 1.0] {
            for _ in 0..2 {
                rows.push((a, b, 10.0 + 2.0 * a + 3.0 * b + a * b));
            }
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
        Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
        Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
    ])
    .expect("frame");
    app.set_data(df, "doe.csv".to_owned());
    app.open_doe_dialog();
    app.doe_response = "y".to_owned();
    app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
    app.run_doe_dialog();
    assert!(app.session_text().contains("2-Level Factorial"));
    assert!(app.session_text().contains("Effect"));
    assert!(app.plot().is_some());
}

#[test]
fn type1_gage_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "m",
        vec![10.1, 10.2, 10.0, 10.3, 10.1, 10.2],
    )])
    .expect("frame");
    app.set_data(df, "type1.csv".to_owned());
    app.open_type1_dialog();
    app.type1_measurement = "m".to_owned();
    app.type1_reference = "10".to_owned();
    app.type1_lsl = "9".to_owned();
    app.type1_usl = "11".to_owned();
    app.run_type1_dialog();
    assert!(app.session_text().contains("Gage Type 1"));
    assert!(app.session_text().contains("%Tolerance"));
}

#[test]
fn gage_rr_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("m", vec![1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 5.0, 6.0]),
        Column::new_text(
            "part",
            vec![
                "A".to_owned(),
                "A".to_owned(),
                "A".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
                "B".to_owned(),
                "B".to_owned(),
                "B".to_owned(),
            ],
        ),
        Column::new_text(
            "operator",
            vec![
                "O1".to_owned(),
                "O1".to_owned(),
                "O2".to_owned(),
                "O2".to_owned(),
                "O1".to_owned(),
                "O1".to_owned(),
                "O2".to_owned(),
                "O2".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "gage.csv".to_owned());
    app.open_gage_dialog();
    app.gage_measurement = "m".to_owned();
    app.gage_part = "part".to_owned();
    app.gage_operator = "operator".to_owned();
    app.run_gage_dialog();
    assert!(app.session_text().contains("Gage R&R"));
    assert!(app.session_text().contains("%Gage R&R"));
}

#[test]
fn kaplan_meier_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        Column::new_f64("status", vec![1.0, 0.0, 1.0, 1.0, 0.0]),
    ])
    .expect("frame");
    app.set_data(df, "survival.csv".to_owned());
    app.open_kaplan_meier_dialog();
    app.km_time = "time".to_owned();
    app.km_censor = Some("status".to_owned());
    app.km_censor_value = "0".to_owned();
    app.run_kaplan_meier_dialog();
    assert!(app.session_text().contains("Kaplan–Meier"));
    assert!(app.session_text().contains("Median survival"));
    assert!(!app.plot().expect("plot").lines.is_empty());
}

#[test]
fn anova_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        Column::new_text(
            "g",
            vec![
                "A".to_owned(),
                "A".to_owned(),
                "B".to_owned(),
                "B".to_owned(),
                "C".to_owned(),
                "C".to_owned(),
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "anova.csv".to_owned());
    app.open_anova_dialog();
    app.anova_response = "y".to_owned();
    app.anova_factor = "g".to_owned();
    app.run_anova_dialog();
    assert!(app.session_text().contains("One-Way ANOVA"));
    assert!(app.session_text().contains("Bonferroni"));
    assert_eq!(app.plot().expect("plot").points.len(), 6);
}

#[test]
fn regression_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("x1", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        Column::new_f64("x2", vec![2.0, 1.0, 4.0, 3.0, 6.0, 5.0]),
        Column::new_f64("y", vec![6.0, 5.0, 12.0, 11.0, 18.0, 17.0]),
    ])
    .expect("frame");
    app.set_data(df, "reg.csv".to_owned());
    app.open_regression_dialog();
    app.reg_response = "y".to_owned();
    app.reg_predictors = vec!["x1".to_owned(), "x2".to_owned()];
    app.run_regression_dialog();
    assert!(app.session_text().contains("Regression"));
    assert!(app.session_text().contains("Regression equation"));
    let plot = app.plot().expect("plot");
    assert_eq!(plot.points.len(), 6);
}

#[test]
fn stepwise_dialog_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64(
            "x1",
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
        ),
        Column::new_f64(
            "x2",
            vec![
                0.769231, -1.188811, 0.853147, -1.104895, 0.937063, -1.020979, 1.020979, -0.937063,
                1.104895, -0.853147, 1.188811, -0.769231,
            ],
        ),
        Column::new_f64(
            "y",
            vec![
                1.410456, 4.198339, 5.105444, 5.594605, 8.019128, 11.014304, 13.465211, 14.88367,
                16.664885, 18.692888, 21.334876, 23.616195,
            ],
        ),
    ])
    .expect("frame");
    app.set_data(df, "step.csv".to_owned());
    app.open_stepwise_dialog();
    app.step_response = "y".to_owned();
    app.step_candidates = vec!["x1".to_owned(), "x2".to_owned()];
    app.step_method = StepMethod::Forward;
    app.step_alpha_enter = 0.05;
    app.run_stepwise_dialog();
    assert!(
        app.session_text().contains("Stepwise Regression"),
        "{}",
        app.session_text()
    );
    assert!(app.session_text().contains("x1"), "{}", app.session_text());
    assert!(!app.session_text().contains("x2"), "{}", app.session_text());
    let plot = app.plot().expect("plot");
    assert_eq!(plot.points.len(), 12);
}

#[test]
fn proportion_and_variance_dialogs_reach_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("a", vec![1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        Column::new_f64("b", vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
    ])
    .expect("frame");
    app.set_data(df, "binary.csv".to_owned());
    app.open_test2_dialog(Test2Kind::OneProportion);
    app.test2_null = "0.5".to_owned();
    app.run_test2_dialog();
    assert!(app.session_text().contains("1 Proportion"));
    app.open_test2_dialog(Test2Kind::TwoProportions);
    app.run_test2_dialog();
    assert!(app.session_text().contains("2 Proportions"));
    app.open_test2_dialog(Test2Kind::OneVariance);
    app.test2_null = "1".to_owned();
    app.run_test2_dialog();
    assert!(app.session_text().contains("1 Variance"));
    app.open_test2_dialog(Test2Kind::TwoVariances);
    app.run_test2_dialog();
    assert!(app.session_text().contains("2 Variances"));
}

#[test]
fn bonett_variance_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("a", vec![10.0, 20.0, 30.0, 40.0, 50.0]),
        Column::new_f64("b", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
    ])
    .expect("frame");
    app.set_data(df, "spread.csv".to_owned());
    app.open_test2_dialog(Test2Kind::TwoVariances);
    app.test2_variance_method = VarianceMethod::Bonett;
    app.run_test2_dialog();
    assert!(app.session_text().contains("Bonett"));
}

#[test]
fn attribute_chart_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("defects", vec![2.0, 4.0, 3.0, 5.0, 1.0, 4.0]),
        Column::new_f64("size", vec![100.0; 6]),
    ])
    .expect("frame");
    app.set_data(df, "defects.csv".to_owned());
    app.open_attribute_dialog();
    app.attr_column = "defects".to_owned();
    app.attr_sizes = Some("size".to_owned());
    app.attr_size.clear();
    app.run_attribute_chart();
    assert!(app.session_text().contains("P Chart"));
    assert_eq!(app.plot().expect("plot").points.len(), 6);
}

#[test]
fn run_chart_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_run_chart("weight");
    assert!(app.session_text().contains("Run Chart"));
    assert!(app.session_text().contains("About the median"));
    let plot = app.plot().expect("plot");
    assert_eq!(plot.points.len(), 5);
    assert_eq!(plot.lines.len(), 1);
}

#[test]
fn control_chart_reaches_the_session_and_plot() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_control_chart("weight", ControlChartKind::IndividualsMovingRange, 1);
    assert!(app.session_text().contains("Control Chart: I–MR"));
    assert!(app.session_text().contains("Control limits"));
    assert_eq!(app.plot().expect("plot").panels.len(), 2);
    app.run_control_chart("weight", ControlChartKind::XbarR, 2);
    assert!(app.session_text().contains("Control Chart: X̄–R"));
}

#[test]
fn control_chart_test_selection_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "weight",
        vec![10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 15.0],
    )])
    .expect("frame");
    app.set_data(df, "weights.csv".to_owned());
    app.run_control_chart_with_tests(
        "weight",
        ControlChartKind::IndividualsMovingRange,
        1,
        TestSet::none(),
    );
    assert!(
        app.session_text().contains("No out-of-control points"),
        "{}",
        app.session_text()
    );
    app.run_control_chart_with_tests(
        "weight",
        ControlChartKind::IndividualsMovingRange,
        1,
        TestSet::only(&[1]).expect("set"),
    );
    assert!(
        app.session_text().contains("point outside a control limit"),
        "{}",
        app.session_text()
    );
    assert!(
        app.session_text().contains("tests 1, subgroup"),
        "{}",
        app.session_text()
    );
    app.run_control_chart_with_tests(
        "weight",
        ControlChartKind::IndividualsMovingRange,
        1,
        TestSet::all().with_sensitivity(2, 8).expect("sensitivity"),
    );
    assert!(
        app.session_text().contains("test 2 K = 8"),
        "{}",
        app.session_text()
    );
}

#[test]
fn descriptive_statistics_by_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![12.5, 12.8, 13.0, 11.9]),
        Column::new_text(
            "batch",
            vec!["A".into(), "A".into(), "B".into(), "B".into()],
        ),
    ])
    .expect("frame");
    app.set_data(df, "grouped.csv".to_owned());
    app.run_descriptive_statistics_by("weight", "batch".to_owned());
    assert!(app.session_text().contains("batch = A"));
    assert!(app.session_text().contains("batch = B"));
    assert!(app.plot().is_none());
}

#[test]
fn box_cox_capability_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.execute_spec(
        AnalysisSpec::capability("weight", Some(8.0), Some(12.0)).with_capability_box_cox(true),
        "box-cox".to_owned(),
    );
    assert!(app.session_text().contains("Normal (Box-Cox)"));
    assert!(app.session_text().contains("Box-Cox λ"));
}

#[test]
fn distribution_comparison_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_distribution_comparison();
    assert!(app.session_text().contains("Distribution Comparison"));
    assert!(app.session_text().contains("Weibull"));
    assert!(app.session_text().contains("AIC"));
}

#[test]
fn descriptive_statistics_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_descriptive_statistics();
    assert!(app.session_text().contains("Descriptive Statistics"));
    assert!(app.session_text().contains("First quartile (Q1)"));
    assert!(app.session_text().contains("95% CI for the mean"));
    assert!(app.plot().is_none());
}

#[test]
fn ryan_joiner_test_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "x",
        vec![-1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1],
    )])
    .expect("frame");
    app.set_data(df, "normal.csv".to_owned());
    app.run_rj_test();
    assert!(app.session_text().contains("Ryan–Joiner"));
    assert!(app.session_text().contains("Monte Carlo RJ"));
}

#[test]
fn kolmogorov_smirnov_test_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![Column::new_f64(
        "x",
        vec![-1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1],
    )])
    .expect("frame");
    app.set_data(df, "normal.csv".to_owned());
    app.run_ks_test();
    assert!(app.session_text().contains("Kolmogorov–Smirnov"));
    assert!(app.session_text().contains("Monte Carlo KS"));
}

#[test]
fn normality_test_adds_a_report_without_a_plot() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_normality_test();
    assert!(app.session_text().contains("Normality Test"));
    assert!(app.session_text().contains("Anderson–Darling"));
    assert!(app.plot().is_none());
}

#[test]
fn series_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=20).map(|index| index as f64 * 0.5).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "series.csv".to_owned());
    app.open_series_dialog();
    app.series_column = "x".to_owned();
    app.run_series_dialog();
    let text = app.session_text();
    assert!(text.contains("Time Series Plot"), "{text}");
    assert!(text.contains("Theil"), "{text}");
    assert!(app.plot().is_some());

    app.open_series_dialog();
    app.series_column = "x".to_owned();
    app.series_time = "x".to_owned();
    app.run_series_dialog();
    assert!(app.status.contains("different"), "{}", app.status);
}

#[test]
fn stem_and_ecdf_dialogs_reach_the_session() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=40).map(|index| 10.0 + index as f64 * 0.5).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());

    app.open_stem_dialog();
    app.stem_column = "x".to_owned();
    app.run_stem_dialog();
    assert!(app.session_text().contains("Stem-and-Leaf"));

    app.open_ecdf_dialog();
    app.ecdf_column = "x".to_owned();
    app.ecdf_confidence = 90.0;
    app.run_ecdf_dialog();
    let text = app.session_text();
    assert!(text.contains("Empirical CDF"), "{text}");
    assert!(text.contains("Band half-width"), "{text}");
    assert!(app.plot().is_some());
}

#[test]
fn dotplot_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut values = Vec::new();
    let mut labels = Vec::new();
    for (index, level) in ["A", "B"].iter().enumerate() {
        for point in 0..5 {
            values.push(10.0 + index as f64 * 2.0 + point as f64 * 0.1);
            labels.push((*level).to_owned());
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("y", values),
        Column::new_text("g", labels),
    ])
    .expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_dotplot_dialog();
    app.dot_column = "y".to_owned();
    app.dot_factor = "g".to_owned();
    app.run_dotplot_dialog();
    let text = app.session_text();
    assert!(text.contains("Dotplot"), "{text}");
    assert_eq!(app.plot().expect("plot").lines.len(), 2);

    app.open_dotplot_dialog();
    app.dot_column = "y".to_owned();
    app.dot_factor = "y".to_owned();
    app.run_dotplot_dialog();
    assert!(app.status.contains("different"), "{}", app.status);
}

#[test]
fn matrix_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut a = Vec::new();
    let mut b = Vec::new();
    for index in 0..20 {
        a.push(index as f64);
        b.push(2.0 * index as f64 + 0.5);
    }
    let df = DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame");
    app.set_data(df, "pairs.csv".to_owned());
    app.open_matrix_dialog();
    app.matrix_columns = "a,b".to_owned();
    app.matrix_bins = 5;
    app.run_matrix_dialog();
    let text = app.session_text();
    assert!(text.contains("Matrix Plot"), "{text}");
    assert!(app.plot().is_some());
    assert_eq!(app.plot().expect("plot").panels.len(), 4);

    app.open_matrix_dialog();
    app.matrix_columns = "a".to_owned();
    app.run_matrix_dialog();
    assert!(app.status.contains("two columns"), "{}", app.status);
}

#[test]
fn interval_dialog_reaches_the_session() {
    let mut app = FreePlotsApp::new();
    let mut values = Vec::new();
    let mut labels = Vec::new();
    for (index, level) in ["A", "B", "C"].iter().enumerate() {
        for point in 0..6 {
            values.push(10.0 + index as f64 * 2.0 + point as f64 * 0.1);
            labels.push((*level).to_owned());
        }
    }
    let df = DataFrame::new(vec![
        Column::new_f64("y", values),
        Column::new_text("g", labels),
    ])
    .expect("frame");
    app.set_data(df, "groups.csv".to_owned());
    app.open_interval_dialog();
    app.interval_response = "y".to_owned();
    app.interval_factor = "g".to_owned();
    app.run_interval_dialog();
    let text = app.session_text();
    assert!(text.contains("Interval Plot"), "{text}");
    assert!(app.plot().is_some());

    app.open_interval_dialog();
    app.interval_factor = "y".to_owned();
    app.run_interval_dialog();
    assert!(app.status.contains("different"), "{}", app.status);
}

#[test]
fn the_tolerance_dialog_offers_the_distribution_free_method() {
    let mut app = FreePlotsApp::new();
    let values: Vec<f64> = (1..=120).map(|index| 40.0 + index as f64 * 0.5).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    app.set_data(df, "values.csv".to_owned());
    app.open_tolerance_dialog();
    app.tolerance_column = "x".to_owned();
    app.tolerance_free = true;
    app.run_tolerance_dialog();
    let text = app.session_text();
    assert!(
        text.contains("Distribution-Free Tolerance Interval"),
        "{text}"
    );
    assert!(text.contains("Achieved confidence"), "{text}");
}

#[test]
fn a_workbook_with_several_sheets_asks_which_one() {
    let mut app = FreePlotsApp::new();
    // A single sheet is opened straight away.
    app.set_workbook_sheets("book.xlsx", vec!["Only".to_owned()]);
    let request = app.take_open_request().expect("request");
    assert_eq!(request.kind, FileKind::Xlsx);
    assert_eq!(request.sheet.as_deref(), Some("Only"));
    // Several sheets open the picker and emit nothing until answered.
    app.set_workbook_sheets("book.xlsx", vec!["First".to_owned(), "Second".to_owned()]);
    assert!(app.take_open_request().is_none());
    assert!(app.show_sheet_dialog);
    assert_eq!(app.sheet_choice, "First");
    app.sheet_choice = "Second".to_owned();
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    output.textures_delta.clear();
    // The dialog is painted; answering it emits the request.
    app.show_sheet_dialog = true;
    app.sheet_path = "book.xlsx".to_owned();
    app.sheet_names = vec!["First".to_owned(), "Second".to_owned()];
    app.sheet_choice = "Second".to_owned();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    output.textures_delta.clear();
}

#[test]
fn open_request_is_taken_once() {
    let mut app = FreePlotsApp::new();
    app.open_request = Some(OpenRequest {
        path: "C:\\data\\weights.csv".to_owned(),
        kind: FileKind::Csv,
        sheet: None,
        csv_options: None,
    });
    let request = app.take_open_request().expect("open request");
    assert_eq!(request.path, "C:\\data\\weights.csv");
    assert_eq!(request.kind, FileKind::Csv);
    assert!(app.take_open_request().is_none());
}

#[test]
fn session_html_export_request_is_exposed() {
    let mut app = FreePlotsApp::new();
    app.begin_session_export();
    assert_eq!(app.save_kind, FileKind::Html);
    assert_eq!(app.save_path, "session.html");
    app.mark_session_exported("session.html");
    assert!(app.status().contains("Session exported"));
}

#[test]
fn save_request_and_mark_saved_are_exposed() {
    let mut app = FreePlotsApp::new();
    app.save_request = Some(SaveRequest {
        path: "out.csv".to_owned(),
        kind: FileKind::Csv,
        decimal_comma: false,
    });
    let request = app.take_save_request().expect("save request");
    assert_eq!(request.path, "out.csv");
    assert_eq!(request.kind, FileKind::Csv);
    assert!(!request.decimal_comma);
    app.mark_saved("out.csv", FileKind::Csv);
    assert_eq!(app.source(), "out.csv");
    assert!(app.status().contains("Saved"));
}

#[test]
fn editing_marks_the_project_dirty_and_a_project_save_clears_it() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    assert!(!app.dirty());
    app.edit_target = Some((0, 0));
    app.edit_buffer = "3".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    assert!(app.dirty());

    app.mark_saved("weights.fpp", FileKind::Project);
    assert!(!app.dirty());
    assert_eq!(app.source(), "weights.fpp");

    // CSV and graph exports are copies, not the project document.
    app.edit_buffer = "4".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    app.mark_saved("weights.csv", FileKind::Csv);
    assert!(app.dirty());
}

#[test]
fn closing_with_unsaved_changes_asks_first() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    app.edit_target = Some((0, 0));
    app.edit_buffer = "3".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());

    assert!(app.request_close());
    assert!(app.close_prompt_open());
    assert!(app.take_recovery_request());
    assert!(!app.take_recovery_request());
    app.cancel_close();
    assert!(!app.close_prompt_open());
    assert!(app.dirty());

    assert!(app.request_close());
    app.discard_and_close();
    assert!(!app.dirty());
    assert!(!app.request_close());
}

#[test]
fn saving_before_close_requests_a_project_write() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    app.edit_target = Some((0, 0));
    app.edit_buffer = "3".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    assert!(app.request_close());
    app.save_before_close();
    assert!(app.show_save);
    assert_eq!(app.save_kind, FileKind::Project);
    assert!(app.save_path.ends_with(".fpp"));
    assert!(app.close_after_save);
    app.mark_saved("weights.fpp", FileKind::Project);
    assert!(!app.dirty());
    app.cancel_close();
    assert!(!app.close_after_save);
}

#[test]
fn the_recovery_file_sits_next_to_the_preferences() {
    let mut app = FreePlotsApp::new();
    let dir = std::env::temp_dir().join(format!("fp-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let expected = dir.join("recovery.fpp");
    app.set_preferences_path(dir.join("settings.json"));
    assert_eq!(app.recovery_path(), Some(expected.as_path()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn autosave_runs_only_when_dirty_and_waits_for_the_interval() {
    let mut app = FreePlotsApp::new();
    let dir = std::env::temp_dir().join(format!("fp-autosave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    app.set_preferences_path(dir.join("settings.json"));
    let expected = dir.join("autosave.fpp");
    assert_eq!(app.autosave_path(), Some(expected.as_path()));
    assert!(!app.autosave_due());

    app.edit_target = Some((0, 0));
    app.edit_buffer = "3".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    assert!(app.autosave_due());
    app.note_autosave();
    assert!(!app.autosave_due());
    app.set_autosave_interval(std::time::Duration::ZERO);
    // No new change: the interval alone does not rewrite the same state.
    assert!(!app.autosave_due());
    app.edit_buffer = "4".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    assert!(app.autosave_due());

    // A project save clears the flag and asks the runner to delete the file.
    app.mark_saved("weights.fpp", FileKind::Project);
    assert!(!app.dirty());
    assert!(app.take_discard_autosave());
    assert!(!app.take_discard_autosave());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn restoring_the_autosave_marks_the_project_dirty() {
    let mut app = FreePlotsApp::new();
    app.offer_restore(std::path::PathBuf::from("autosave.fpp"));
    assert!(app.restore_prompt_open());
    app.apply_restored(
        vec![("Worksheet 1".to_owned(), frame())],
        "autosave.fpp [recovered]".to_owned(),
    );
    assert!(!app.restore_prompt_open());
    assert!(app.dirty());
    assert_eq!(app.source(), "autosave.fpp [recovered]");
    assert_eq!(app.save_kind, FileKind::Project);
    assert_eq!(app.save_path, "recovered.fpp");
    assert!(app.status().contains("Recovered"));
}

#[test]
fn the_restore_prompt_offers_restore_discard_and_later() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.offer_restore(std::path::PathBuf::from("autosave.fpp"));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Restore");
    harness.get_by_label("Discard");
    harness.get_by_label("Later").click();
    harness.run();
    assert!(!harness.state().restore_prompt_open());
}

#[test]
fn the_restore_prompt_follows_the_language() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_language(fp_core::i18n::Lang::Portuguese);
    app.offer_restore(std::path::PathBuf::from("autosave.fpp"));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Recuperar trabalho não guardado");
    harness.get_by_label("Restaurar");
    harness.get_by_label("Descartar");
    harness.get_by_label("Mais tarde");
}

#[test]
fn the_close_prompt_offers_save_discard_and_cancel() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    app.edit_target = Some((0, 0));
    app.edit_buffer = "3".to_owned();
    app.edit_dirty = true;
    assert!(app.commit_cell());
    assert!(app.request_close());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Save");
    harness.get_by_label("Discard");
    harness.get_by_label("Cancel");
}

#[test]
fn project_save_uses_the_fpp_extension() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
    app.begin_save(FileKind::Project);
    assert!(app.show_save);
    assert_eq!(app.save_kind, FileKind::Project);
    assert_eq!(app.save_path, "weights.fpp");
    app.begin_save(FileKind::Csv);
    assert_eq!(app.save_path, "weights.csv");
}

#[test]
fn new_worksheet_starts_editable_with_eight_columns() {
    let mut app = FreePlotsApp::new();
    app.new_worksheet();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(8));
    assert_eq!(app.data().map(DataFrame::n_rows), Some(0));
    assert_eq!(app.selected_cell(), Some((0, 0)));

    app.set_edit_buffer("10,5");
    assert!(app.commit_cell());
    assert_eq!(app.data().map(DataFrame::n_rows), Some(1));
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("10.5"));
}

#[test]
fn worksheet_editing_reaches_the_data_frame() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 1);
    assert_eq!(app.edit_buffer(), "13.1");
    app.set_edit_buffer("14,25");
    assert!(app.commit_cell());
    assert_eq!(
        app.data().unwrap().cell_text(0, 1).as_deref(),
        Some("14.25")
    );
    assert!(app.column_summary().contains("mean="));
}

#[test]
fn rectangle_selection_normalizes_and_copies_tsv() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.extend_selection(1, 1);
    assert_eq!(app.selection_bounds(), Some((0, 0, 1, 1)));
    assert_eq!(app.selection_size(), (2, 2));
    assert_eq!(app.selection_tsv().as_deref(), Some("12.5\tA\n13.1\tA"));

    app.select_cell(1, 2);
    app.extend_selection(0, 0);
    assert_eq!(app.selection_bounds(), Some((0, 0, 1, 2)));
}

#[test]
fn missing_cells_copy_as_empty_fields() {
    let mut app = FreePlotsApp::new();
    let df = DataFrame::new(vec![
        Column::new_f64("x", vec![1.0, f64::NAN]),
        Column::new_text("y", vec!["a".into(), String::new()]),
    ])
    .unwrap();
    app.set_data(df, "missing.csv".to_owned());
    app.select_cell(0, 0);
    app.extend_selection(1, 1);
    assert_eq!(app.selection_tsv().as_deref(), Some("1\ta\n\t"));
}

#[test]
fn clearing_a_rectangle_is_one_undo_group() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.extend_selection(1, 1);
    app.clear_selection();
    assert!(app.data().unwrap().cell_is_missing(0, 0));
    assert!(app.data().unwrap().cell_is_missing(1, 1));
    assert!(!app.data().unwrap().cell_is_missing(0, 2));
    assert_eq!(app.selection_size(), (2, 2));
    app.undo();
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    assert_eq!(app.data().unwrap().cell_text(1, 1).as_deref(), Some("A"));
}

#[test]
fn inline_edit_commits_and_is_undoable() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.begin_inline_edit(0, 1);
    assert_eq!(app.inline_edit, Some((0, 1)));
    assert_eq!(app.inline_buffer, "13.1");
    app.inline_buffer = "15.25".to_owned();
    assert!(app.commit_inline_edit());
    assert!(app.inline_edit.is_none());
    assert_eq!(
        app.data().unwrap().cell_text(0, 1).as_deref(),
        Some("15.25")
    );
    app.undo();
    assert_eq!(app.data().unwrap().cell_text(0, 1).as_deref(), Some("13.1"));
}

#[test]
fn inline_edit_of_missing_cell_starts_empty_and_cancel_keeps_it_missing() {
    let mut app = FreePlotsApp::new();
    app.new_worksheet();
    app.begin_inline_edit(0, 0);
    assert!(app.inline_buffer.is_empty());
    app.inline_buffer = "1".to_owned();
    app.cancel_inline_edit();
    assert!(app.inline_edit.is_none());
    assert!(app.data().unwrap().columns()[0].is_missing(0));
}

#[test]
fn cell_edits_can_be_undone_and_redone() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.set_edit_buffer("99");
    assert!(app.commit_cell());
    assert!(app.can_undo());

    app.undo();
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    assert!(app.can_redo());

    app.redo();
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));
}

#[test]
fn undo_removes_a_row_created_by_the_edit() {
    let mut app = FreePlotsApp::new();
    app.new_worksheet();
    app.select_cell(0, 5);
    app.set_edit_buffer("7");
    assert!(app.commit_cell());
    assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
    app.undo();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(0));
}

#[test]
fn paste_that_adds_columns_is_undone_as_one_group() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(1, 0);
    app.paste_tsv("1\t2\t3");
    assert_eq!(app.data().map(DataFrame::n_columns), Some(4));
    assert!(app.can_undo(), "nothing to undo: {}", app.status());
    app.undo();
    assert_eq!(
        app.data().map(DataFrame::n_columns),
        Some(2),
        "status: {}",
        app.status()
    );
    assert_eq!(app.data().unwrap().cell_text(1, 0).as_deref(), Some("A"));
}

#[test]
fn paste_is_undone_as_one_group() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.paste_tsv("1\t2\n3\t4");
    assert_eq!(app.data().unwrap().cell_text(1, 1).as_deref(), Some("4"));
    app.undo();
    let df = app.data().unwrap();
    assert_eq!(df.cell_text(0, 0).as_deref(), Some("12.5"));
    assert_eq!(df.cell_text(1, 1).as_deref(), Some("A"));
}

#[test]
fn structural_edits_are_undoable() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());

    app.add_row();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
    app.undo();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    app.redo();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(6));

    app.select_cell(0, 0);
    app.delete_row();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    app.undo();
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));

    app.add_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
    app.undo();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
    app.redo();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(3));

    app.select_cell(1, 0);
    app.delete_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
    app.undo();
    assert_eq!(app.data().unwrap().cell_text(1, 0).as_deref(), Some("A"));

    app.select_cell(0, 0);
    app.begin_rename_column();
    app.rename_text = "mass".to_owned();
    app.commit_rename_column();
    assert_eq!(app.data().unwrap().columns()[0].name(), "mass");
    app.undo();
    assert_eq!(app.data().unwrap().columns()[0].name(), "weight");
    app.redo();
    assert_eq!(app.data().unwrap().columns()[0].name(), "mass");
}

#[test]
fn undo_and_redo_follow_the_order_of_cell_and_structural_edits() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.set_edit_buffer("99");
    assert!(app.commit_cell());
    app.add_row();
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));

    app.undo(); // remove the row first
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));
    app.undo(); // then restore the cell
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
}

#[test]
fn invalid_cell_text_is_rejected_without_losing_the_old_value() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.set_edit_buffer("not numeric");
    assert!(!app.commit_cell());
    assert!(app.status().contains("expected a number"));
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
}

#[test]
fn add_remove_rows_and_columns() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_row();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
    app.add_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
    app.add_text_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(4));
    assert_eq!(app.data().unwrap().columns()[3].dtype(), DType::Text);
    app.delete_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
    app.delete_column();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
    app.delete_row();
    assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
}

#[test]
fn paste_tsv_fills_a_rectangle() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.paste_tsv("1\t2\n3\t4");
    let df = app.data().unwrap();
    assert_eq!(df.cell_text(0, 0).as_deref(), Some("1"));
    assert_eq!(df.cell_text(1, 1).as_deref(), Some("4"));
}

#[test]
fn rename_column_updates_the_selection() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(1, 0);
    app.begin_rename_column();
    app.rename_text = "batch_id".to_owned();
    app.commit_rename_column();
    assert_eq!(app.data().unwrap().columns()[1].name(), "batch_id");
    assert_eq!(app.selected_column(), Some("batch_id"));
}

#[test]
#[ignore = "phase 1 grid budget; run on the target machine"]
fn grid_100k_by_20_stays_inside_the_frame_budget() {
    use std::time::{Duration, Instant};

    let rows = 100_000;
    let columns = 20;
    let values: Vec<Vec<f64>> = (0..columns)
        .map(|column| {
            (0..rows)
                .map(|row| (row as f64) * 0.001 + column as f64)
                .collect()
        })
        .collect();
    let df = DataFrame::new(
        values
            .into_iter()
            .enumerate()
            .map(|(index, values)| Column::new_f64(format!("C{}", index + 1), values))
            .collect(),
    )
    .expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "large.csv".to_owned());

    let ctx = egui::Context::default();
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1320.0, 840.0),
        )),
        ..Default::default()
    };
    // Warm up fonts and layout caches.
    let mut output = ctx.run_ui(input(), |ui| app.show(ui));
    output.textures_delta.clear();

    let started = Instant::now();
    for _ in 0..3 {
        let mut output = ctx.run_ui(input(), |ui| app.show(ui));
        output.textures_delta.clear();
    }
    let frame = started.elapsed() / 3;
    // The 20 ms budget is the desktop target; the Raspberry Pi 4 and the
    // shared CI runner get a wider budget for the same frame.
    let frame_budget_ms = std::env::var("FREEPLOTS_FRAME_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(if cfg!(target_arch = "aarch64") {
            60
        } else {
            20
        });
    let frame_budget = Duration::from_millis(frame_budget_ms);
    println!("100k×20 frame: {frame:?} (budget {frame_budget:?})");
    assert!(frame <= frame_budget, "frame took {frame:?}");

    app.select_cell(0, 50_000);
    app.set_edit_buffer("42");
    let started = Instant::now();
    assert!(app.commit_cell());
    let edit = started.elapsed();
    let edit_budget_ms = std::env::var("FREEPLOTS_EDIT_BUDGET_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(if cfg!(target_arch = "aarch64") {
            40
        } else {
            16
        });
    let edit_budget = Duration::from_millis(edit_budget_ms);
    println!("cell edit + status summary: {edit:?} (budget {edit_budget:?})");
    assert!(edit <= edit_budget, "edit took {edit:?}");
}

#[test]
fn headless_frame_paints_worksheet_and_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_probability_plot();

    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    let painted = output.shapes.len();
    output.textures_delta.clear();
    assert!(painted > 0, "the frame painted nothing");
}

#[test]
fn toolbar_renders_the_icon_buttons() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    let painted = output.shapes.len();
    output.textures_delta.clear();
    assert!(painted > 0, "the toolbar painted nothing");
}

#[test]
fn every_tool_icon_paints_inside_its_box() {
    // One representative of every family the menus use; each must draw
    // something visible and stay inside its 26×24 hit area.
    let icons = [
        ToolIcon::Open,
        ToolIcon::Save,
        ToolIcon::New,
        ToolIcon::Export,
        ToolIcon::Undo,
        ToolIcon::Redo,
        ToolIcon::Plot,
        ToolIcon::Clear,
        ToolIcon::Pencil,
        ToolIcon::Calculator,
        ToolIcon::Grid,
        ToolIcon::Plus,
        ToolIcon::Minus,
        ToolIcon::Sort,
        ToolIcon::Swap,
        ToolIcon::Layers,
        ToolIcon::Filter,
        ToolIcon::Box,
        ToolIcon::Bars,
        ToolIcon::Dots,
        ToolIcon::Info,
        ToolIcon::Globe,
        ToolIcon::Plug,
        ToolIcon::Sigma,
        ToolIcon::Clock,
    ];
    for icon in icons {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_icon(
                ui.painter(),
                egui::pos2(20.0, 20.0),
                icon,
                egui::Stroke::new(1.4, egui::Color32::BLACK),
            );
        });
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty(), "an icon painted nothing");
        let bounds = output
            .shapes
            .iter()
            .fold(egui::Rect::NOTHING, |rect, shape| {
                rect.union(shape.shape.visual_bounding_rect())
            });
        assert!(
            bounds.width().max(bounds.height()) >= 6.0,
            "an icon is too small to see: {bounds:?}"
        );
        assert!(
            bounds.width() <= 26.0 && bounds.height() <= 24.0,
            "an icon overflows its hit area: {bounds:?}"
        );
    }
}

#[test]
fn kittest_opens_the_about_dialog_through_the_menu() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), FreePlotsApp::new());
    harness.run();
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("About FreePlots").click();
    harness.run();
    assert!(harness.state().show_about);
}

#[test]
fn analysis_without_data_reports_an_error() {
    let mut app = FreePlotsApp::new();
    app.run_probability_plot();
    assert!(app.status().contains("worksheet is empty"));
}

#[test]
fn the_native_plot_window_option_is_on_by_default() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert!(harness.state().native_plot_windows);
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Native plot windows").click();
    harness.run();
    assert!(!harness.state().native_plot_windows);
}

#[test]
fn the_native_plot_viewport_embeds_in_the_test_harness() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.native_plot_windows = true;
    app.run_probability_plot();
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
    output.textures_delta.clear();
    assert!(app.show_plot);
}

#[test]
fn a_warning_reaches_the_session_and_the_status() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.push_warning("2 formula cells were read as cached values");
    assert!(
        app.session_text()
            .contains("Warning: 2 formula cells were read as cached values")
    );
    assert!(app.status().contains("cached values"));
}

#[test]
fn the_clear_button_resets_the_transient_dialog_options() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.open_probability_plot_dialog();
    app.dialog_group = Some("batch".to_owned());
    app.dialog_display = PlotDisplay::Symbols;
    app.dialog_ci = false;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    {
        let buttons: Vec<_> = harness.get_all_by_label("Clear").collect();
        assert!(!buttons.is_empty());
        // The dialog's Clear is the left one; the Session panel also has a
        // Clear button at the top right.
        let button = buttons
            .into_iter()
            .min_by(|a, b| a.rect().center().x.total_cmp(&b.rect().center().x))
            .expect("clear");
        button.click();
    }
    harness.run();
    harness.run();
    let app = harness.state();
    assert!(app.dialog_group.is_none());
    // Presentation choices are preferences now, so Clear keeps them.
    assert_eq!(app.dialog_display, PlotDisplay::Symbols);
    assert!(!app.dialog_ci);
    assert!(app.show_probability_plot_dialog);
}

#[test]
fn the_csv_preview_confirms_the_detected_options() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_csv_preview(
        "weights.csv".to_owned(),
        CsvPreview {
            text: "weight;height\n12,5;10\n13,5;11".to_owned(),
            options: CsvOptions {
                delimiter: b';',
                decimal_comma: true,
                has_header: true,
            },
            encoding: "UTF-8".to_owned(),
        },
    );
    assert!(app.show_csv_options);
    assert_eq!(app.csv_delimiter, b';');
    assert!(app.csv_decimal_comma);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Open").click();
    harness.run();
    let request = harness
        .state_mut()
        .take_open_request()
        .expect("open request");
    assert_eq!(request.kind, FileKind::Csv);
    assert_eq!(
        request.csv_options,
        Some(CsvOptions {
            delimiter: b';',
            decimal_comma: true,
            has_header: true,
        })
    );
    assert!(!harness.state().show_csv_options);
}

#[test]
fn the_date_format_changes_the_grid_text() {
    let date = |text: &str| fp_core::data::parse_date(text).expect("date");
    let mut app = FreePlotsApp::new();
    app.set_data(
        DataFrame::new(vec![Column::new_date(
            "date",
            vec![date("2024-02-29"), date("2025-01-01")],
        )])
        .expect("frame"),
        "dates.csv".to_owned(),
    );
    app.date_format = DateFormat::DayMonthYear;
    assert_eq!(
        app.display_cell(app.data().expect("data"), 0, 0),
        "29/02/2024"
    );
    app.date_format = DateFormat::DayMonthNameYear;
    assert_eq!(
        app.display_cell(app.data().expect("data"), 0, 1),
        "01 Jan 2025"
    );
    app.date_format = DateFormat::Iso;
    assert_eq!(
        app.display_cell(app.data().expect("data"), 0, 0),
        "2024-02-29"
    );
}

#[test]
fn the_help_catalog_covers_the_common_controls() {
    for label in [
        "Column:",
        "Distribution:",
        "Confidence:",
        "Response:",
        "LSL:",
        "Time column:",
        "Use stress:",
        "Censoring value:",
        "Sample size:",
        "Keys:",
        "New name:",
    ] {
        assert!(label_help(label).is_some(), "{label}");
    }
    assert!(label_help("Some unknown field:").is_some());
    assert!(label_help("A plain heading").is_none());
    assert!(button_help("Run").contains("analysis"));
    assert!(button_help("Cancel").contains("without"));
    assert!(checkbox_help("Randomize run order").contains("seed"));
}

#[test]
fn find_selects_the_next_match_and_wraps_around() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.find_text = "13.1".to_owned();
    app.find_next();
    assert_eq!(app.selected_cell(), Some((0, 1)));
    assert_eq!(app.find_message, "Found at C1 R2");
    // Wrapping around finds the same single occurrence again.
    app.find_next();
    assert_eq!(app.find_message, "Found at C1 R2");
    app.find_text = "missing".to_owned();
    app.find_next();
    assert_eq!(app.find_message, "No matches");
}

#[test]
fn find_options_respect_case_and_whole_cells() {
    assert!(text_matches("Alpha", "alpha", false, false));
    assert!(!text_matches("Alpha", "alpha", true, false));
    assert!(text_matches("Alpha", "Alpha", true, true));
    assert!(!text_matches("Alphabet", "Alpha", true, true));
    assert_eq!(
        replace_in("a-B-a", "a", "x", false, false).as_deref(),
        Some("x-B-x")
    );
    assert_eq!(replace_in("a-B-a", "a", "x", true, true), None);
    assert_eq!(replace_in("a", "a", "x", true, true).as_deref(), Some("x"));
}

#[test]
fn replace_all_rewrites_matches_and_undo_restores_them() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.find_text = "A".to_owned();
    app.replace_text = "B".to_owned();
    app.replace_all();
    assert_eq!(app.find_message, "5 replaced");
    for row in 0..5 {
        assert_eq!(
            app.data().expect("frame").cell_text(1, row).as_deref(),
            Some("B")
        );
    }
    app.undo();
    for row in 0..5 {
        assert_eq!(
            app.data().expect("frame").cell_text(1, row).as_deref(),
            Some("A")
        );
    }
}

#[test]
fn fill_down_and_right_copy_the_first_row_and_column() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.extend_selection(0, 2);
    app.fill_down();
    assert_eq!(
        app.data().expect("frame").cell_text(0, 1).as_deref(),
        Some("12.5")
    );
    assert_eq!(
        app.data().expect("frame").cell_text(0, 2).as_deref(),
        Some("12.5")
    );
    app.undo();

    app.select_cell(0, 0);
    app.extend_selection(1, 0);
    app.fill_right();
    assert_eq!(
        app.data().expect("frame").cell_text(1, 0).as_deref(),
        Some("12.5")
    );
    app.undo();
    assert_eq!(
        app.data().expect("frame").cell_text(1, 0).as_deref(),
        Some("A")
    );
}

#[test]
fn a_column_format_changes_the_display_only() {
    let format = ColumnFormat {
        decimals: 2,
        thousands: true,
        bold: true,
        align: CellAlign::Right,
    };
    assert_eq!(
        format_cell_text("1234.5678".to_owned(), Some(format)),
        "1 234.57"
    );
    assert_eq!(
        format_cell_text("-1234.5".to_owned(), Some(format)),
        "-1 234.50"
    );
    assert_eq!(format_cell_text("text".to_owned(), Some(format)), "text");
    assert_eq!(format_cell_text("12.5".to_owned(), None), "12.5");

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.formats.insert("weight".to_owned(), format);
    assert_eq!(
        app.data().expect("frame").cell_text(0, 0).as_deref(),
        Some("12.5")
    );
    app.clear_column_format();
    assert!(app.formats.is_empty());
}

#[test]
fn dropping_a_file_picks_the_reader_by_extension() {
    let mut app = FreePlotsApp::new();
    assert!(app.open_dropped("/tmp/report.csv"));
    assert_eq!(app.preview_request.as_deref(), Some("/tmp/report.csv"));
    assert!(app.open_dropped("/tmp/book.XLSX"));
    assert_eq!(
        app.open_request.as_ref().map(|request| request.kind),
        Some(FileKind::Xlsx)
    );
    app.open_request = None;
    assert!(app.open_dropped("/tmp/session.fpp"));
    assert_eq!(
        app.open_request.as_ref().map(|request| request.kind),
        Some(FileKind::Project)
    );
    assert!(!app.open_dropped("/tmp/notes.pdf"));
}

#[test]
fn date_difference_and_shift_add_columns() {
    let date = |text: &str| fp_core::data::parse_date(text).expect("date");
    let mut app = FreePlotsApp::new();
    app.set_data(
        DataFrame::new(vec![
            Column::new_date("start", vec![date("2024-01-01"), date("2024-02-01")]),
            Column::new_date("end", vec![date("2024-01-11"), date("2024-03-01")]),
        ])
        .expect("frame"),
        "dates.csv".to_owned(),
    );
    app.open_date_diff_dialog();
    assert_eq!(app.date_diff_first, "start");
    assert_eq!(app.date_diff_second, "end");
    app.run_date_diff_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("frame");
    assert_eq!(df.n_columns(), 3);
    assert_eq!(df.cell_text(2, 0).as_deref(), Some("10"));
    assert_eq!(df.cell_text(2, 1).as_deref(), Some("29"));

    app.open_date_shift_dialog();
    assert_eq!(app.date_shift_column, "start");
    app.date_shift_days = 0;
    app.date_shift_name = "same".to_owned();
    app.run_date_shift_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("frame");
    assert_eq!(df.cell_text(3, 0), df.cell_text(0, 0));

    app.open_date_shift_dialog();
    app.date_shift_column = "start".to_owned();
    app.date_shift_days = -1;
    app.date_shift_name = "before".to_owned();
    app.run_date_shift_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
}

#[test]
fn the_date_dialogs_reject_missing_columns() {
    let mut app = FreePlotsApp::new();
    app.open_date_diff_dialog();
    app.date_diff_first.clear();
    app.date_diff_second.clear();
    app.run_date_diff_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the two date columns"));
    app.date_diff_first = "a".to_owned();
    app.date_diff_second = "b".to_owned();
    app.date_diff_name.clear();
    app.run_date_diff_dialog();
    assert_eq!(app.error.as_deref(), Some("Name the new column"));

    app.open_date_shift_dialog();
    app.date_shift_column.clear();
    app.run_date_shift_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the date column"));
}

#[test]
fn the_step_stress_dialog_needs_every_column() {
    let mut app = FreePlotsApp::new();
    app.open_step_stress_dialog();
    app.ss_duration.clear();
    app.ss_stress.clear();
    app.ss_unit.clear();
    app.ss_event.clear();
    app.run_step_stress_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select the duration, stress, unit and event columns")
    );
}

#[test]
fn the_step_stress_dialog_fits_a_synthetic_test() {
    let mut app = FreePlotsApp::new();
    app.set_data(step_stress_frame(), "step.csv".to_owned());
    app.open_step_stress_dialog();
    // The first columns are picked in declaration order.
    app.ss_duration = "duration".to_owned();
    app.ss_stress = "stress".to_owned();
    app.ss_unit = "unit".to_owned();
    app.ss_event = "event".to_owned();
    app.ss_use_stress = 90.0;
    app.run_step_stress_dialog();
    let text = app.session_text();
    assert!(
        app.error.is_none() || text.is_empty(),
        "unexpected error: {:?}",
        app.error
    );
}

#[test]
fn the_desirability_dialog_validates_and_optimises() {
    let mut app = FreePlotsApp::new();
    app.set_data(desirability_frame(), "design.csv".to_owned());

    app.open_desirability_dialog();
    app.desirability_factors = vec!["a".to_owned()];
    app.run_desirability_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select at least two factor columns")
    );

    app.open_desirability_dialog();
    app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
    app.run_desirability_dialog();
    assert_eq!(app.error.as_deref(), Some("Add at least one response"));

    app.open_desirability_dialog();
    app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
    app.desirability_responses.push(DesirabilityRow {
        column: "y".to_owned(),
        goal: Goal::Maximize,
        low: "0".to_owned(),
        target: "0".to_owned(),
        high: "30".to_owned(),
        weight: "1".to_owned(),
        importance: "1".to_owned(),
    });
    app.desirability_responses[0].goal = Goal::Target;
    app.desirability_responses[0].target = "not a number".to_owned();
    app.run_desirability_dialog();
    assert_eq!(app.error.as_deref(), Some("Target must be a number"));
}

#[test]
fn the_response_optimiser_runs_on_a_quadratic_design() {
    let mut app = FreePlotsApp::new();
    app.set_data(desirability_frame(), "design.csv".to_owned());
    app.plot = None;
    app.open_desirability_dialog();
    app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
    app.desirability_responses.push(DesirabilityRow {
        column: "y".to_owned(),
        goal: Goal::Maximize,
        low: "0".to_owned(),
        target: String::new(),
        high: "30".to_owned(),
        weight: "1".to_owned(),
        importance: "1".to_owned(),
    });
    app.run_desirability_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Response Optimizer"));
}

#[test]
fn control_chart_options_reach_the_session() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_control_chart_with_options(
        "weight",
        ControlChartKind::IndividualsMovingRange,
        1,
        TestSet::default(),
        fp_core::spc::CusumOptions::default(),
        fp_core::spc::EwmaOptions::default(),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Control Chart"));
    assert!(app.plot().is_some());
}

#[test]
fn run_spec_reports_the_empty_worksheet() {
    let mut app = FreePlotsApp::new();
    app.data = None;
    let result = app.run_spec(
        AnalysisSpec::capability("x", Some(1.0), None),
        "Capability".to_owned(),
    );
    assert!(result.is_err());
    assert_eq!(
        app.error.as_deref(),
        Some("No worksheet open: use File → Open CSV…")
    );
}

#[test]
fn the_find_and_format_dialogs_open_with_the_current_values() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.open_find_dialog();
    assert!(app.show_find_dialog);
    app.open_format_dialog();
    assert!(app.show_format_dialog);
    assert_eq!(app.format_decimals, 6);
    assert_eq!(app.format_align, CellAlign::Left);
    app.format_decimals = 2;
    app.format_thousands = true;
    app.format_bold = true;
    app.format_align = CellAlign::Right;
    app.apply_column_format();
    let saved = app.formats.get("weight").copied().expect("format");
    assert_eq!(saved.decimals, 2);
    assert!(saved.thousands && saved.bold);
    assert_eq!(saved.align, CellAlign::Right);
    app.open_format_dialog();
    assert_eq!(app.format_decimals, 2);
    assert_eq!(app.format_align, CellAlign::Right);
}

#[test]
fn the_t_test_dialog_honours_its_options() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "samples.csv".to_owned());

    app.open_t_test_dialog(TTestKind::OneSample);
    assert!(app.show_t_dialog);
    app.t_first = "before".to_owned();
    app.t_null = "12".to_owned();
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("1-Sample t"));
    let two_sided = app.session_text().to_owned();

    app.session.clear();
    app.t_alternative = Alternative::Greater;
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(
        app.session_text(),
        two_sided,
        "the alternative changed nothing"
    );

    app.session.clear();
    app.t_confidence = 99.9;
    app.t_alternative = Alternative::TwoSided;
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(
        app.session_text(),
        two_sided,
        "the confidence changed nothing"
    );

    app.session.clear();
    app.open_t_test_dialog(TTestKind::TwoSample);
    app.t_first = "before".to_owned();
    app.t_second = "after".to_owned();
    app.t_pooled = true;
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let pooled = app.session_text().to_owned();
    app.session.clear();
    app.open_t_test_dialog(TTestKind::TwoSample);
    app.t_first = "before".to_owned();
    app.t_second = "after".to_owned();
    app.t_pooled = false;
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), pooled, "pooling changed nothing");

    app.session.clear();
    app.open_t_test_dialog(TTestKind::Paired);
    app.t_first = "before".to_owned();
    app.t_second = "after".to_owned();
    app.run_t_test_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Paired t"));
}

#[test]
fn the_t_test_dialog_rejects_bad_input() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "samples.csv".to_owned());
    app.open_t_test_dialog(TTestKind::OneSample);
    app.t_first.clear();
    app.run_t_test_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select the first numeric column")
    );
    app.t_first = "before".to_owned();
    app.t_null = "abc".to_owned();
    app.run_t_test_dialog();
    assert_eq!(app.error.as_deref(), Some("Null mean must be a number"));
    app.open_t_test_dialog(TTestKind::TwoSample);
    app.t_first = "before".to_owned();
    app.t_second.clear();
    app.run_t_test_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select the second numeric column")
    );
    app.t_second = "before".to_owned();
    app.run_t_test_dialog();
    assert_eq!(app.error.as_deref(), Some("Select two different columns"));
}

#[test]
fn the_equivalence_dialog_checks_the_margin() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "samples.csv".to_owned());
    app.open_equivalence_dialog(EquivalenceKind::TwoSample);
    app.equivalence_first = "before".to_owned();
    app.equivalence_second = "after".to_owned();
    app.equivalence_margin = "1.5".to_owned();
    app.run_equivalence_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let text = app.session_text().to_owned();
    assert!(text.to_lowercase().contains("equivalence"), "{text}");

    app.session.clear();
    app.open_equivalence_dialog(EquivalenceKind::TwoSample);
    app.equivalence_first = "before".to_owned();
    app.equivalence_second = "after".to_owned();
    app.equivalence_margin = "abc".to_owned();
    app.run_equivalence_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The equivalence margin must be a number")
    );
    app.equivalence_margin = "0".to_owned();
    app.run_equivalence_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The equivalence margin must be positive")
    );
}

#[test]
fn the_proportion_dialog_honours_the_method_and_rejects_bad_values() {
    let mut app = FreePlotsApp::new();
    app.set_data(proportion_frame(), "pass.csv".to_owned());

    app.open_test2_dialog(Test2Kind::OneProportion);
    app.test2_first = "pass".to_owned();
    app.test2_null = "0.5".to_owned();
    app.test2_success = "1".to_owned();
    app.test2_method = ProportionMethod::Exact;
    app.run_test2_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let exact = app.session_text().to_owned();
    assert!(exact.contains("1 Proportion"), "{exact}");

    app.session.clear();
    app.test2_method = ProportionMethod::Normal;
    app.run_test2_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), exact, "the method changed nothing");

    app.test2_null = "abc".to_owned();
    app.run_test2_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Null proportion must be a number")
    );

    app.session.clear();
    app.open_test2_dialog(Test2Kind::TwoProportions);
    app.test2_first = "pass".to_owned();
    app.test2_success = "1".to_owned();
    app.test2_second.clear();
    app.run_test2_dialog();
    assert_eq!(app.error.as_deref(), Some("Select two different columns"));
    app.test2_second = "retest".to_owned();
    app.run_test2_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("2 Proportions"));
}

#[test]
fn the_power_dialog_solves_for_n_and_validates() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "samples.csv".to_owned());
    app.power_procedure = PowerProcedure::TwoSampleT;
    app.power_effect = "0.5".to_owned();
    app.power_n = "0".to_owned();
    app.power_target = 80.0;
    app.power_alpha = 5.0;
    app.power_groups = 2;
    app.run_power_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Power"));

    app.session.clear();
    app.power_procedure = PowerProcedure::TwoSampleT;
    app.power_effect = "0.5".to_owned();
    app.power_n = "50".to_owned();
    app.run_power_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Power"));

    app.power_effect = "abc".to_owned();
    app.run_power_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The effect size must be a number")
    );
    app.power_effect = "0.5".to_owned();
    app.power_n = "abc".to_owned();
    app.run_power_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The sample size must be a whole number (0 solves for it)")
    );
}

#[test]
fn every_control_chart_kind_runs() {
    let mut app = FreePlotsApp::new();
    app.set_data(process_frame(), "process.csv".to_owned());
    let kinds = [
        ControlChartKind::IndividualsMovingRange,
        ControlChartKind::XbarR,
        ControlChartKind::XbarS,
        ControlChartKind::Ewma,
        ControlChartKind::Cusum,
    ];
    for kind in kinds {
        app.session.clear();
        app.run_control_chart_with_options(
            "x",
            kind,
            5,
            TestSet::default(),
            fp_core::spc::CusumOptions::default(),
            fp_core::spc::EwmaOptions::default(),
        );
        assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
        assert!(
            app.session_text().contains(kind.name()),
            "{} missing from the session",
            kind.name()
        );
    }
}

#[test]
fn the_control_chart_tests_and_options_are_applied() {
    let mut app = FreePlotsApp::new();
    app.set_data(process_frame(), "process.csv".to_owned());
    let tests = TestSet::only(&[1, 2]).expect("tests");
    let cusum = fp_core::spc::CusumOptions::new(0.5, 4.0).expect("cusum");
    let ewma = fp_core::spc::EwmaOptions::new(0.2, 3.0).expect("ewma");
    app.run_control_chart_with_options(
        "x",
        ControlChartKind::IndividualsMovingRange,
        1,
        tests,
        cusum,
        ewma,
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.plot().is_some());

    // Out-of-range options are rejected before the chart is drawn.
    assert!(fp_core::spc::CusumOptions::new(0.0, 4.0).is_err());
    assert!(fp_core::spc::CusumOptions::new(0.5, 0.0).is_err());
    assert!(fp_core::spc::EwmaOptions::new(0.0, 3.0).is_err());
    assert!(fp_core::spc::EwmaOptions::new(1.5, 3.0).is_err());
    assert!(fp_core::spc::TestSet::only(&[0]).is_err());
    assert!(fp_core::spc::TestSet::only(&[9]).is_err());
}

#[test]
fn the_capability_options_reach_the_engine() {
    let mut app = FreePlotsApp::new();
    app.set_data(process_frame(), "process.csv".to_owned());

    let normal = AnalysisSpec::capability("x", Some(8.0), Some(12.0));
    app.run_spec(normal, "Capability".to_owned())
        .expect("normal capability");
    let normal_text = app.session_text().to_owned();
    assert!(normal_text.contains("Capability"));

    app.session.clear();
    let lognormal = AnalysisSpec::capability("x", Some(8.0), Some(12.0))
        .with_capability_distribution(DistributionKind::Lognormal)
        .with_capability_bootstrap(50)
        .with_capability_target(Some(10.0));
    app.run_spec(lognormal, "Capability".to_owned())
        .expect("lognormal capability");
    assert_ne!(app.session_text(), normal_text);

    app.session.clear();
    let box_cox = AnalysisSpec::capability("x", Some(8.0), None).with_capability_box_cox(true);
    app.run_spec(box_cox, "Capability".to_owned())
        .expect("box-cox capability");

    // Requiring a stable screen refuses this drifting series.
    app.session.clear();
    let strict = AnalysisSpec::capability("x", Some(8.0), None).with_capability_stability(true);
    assert!(app.run_spec(strict, "Capability".to_owned()).is_err());
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("special-cause")),
        "{:?}",
        app.error
    );

    // No specification limit is refused by the engine.
    let none = AnalysisSpec::capability("x", None, None);
    assert!(app.run_spec(none, "Capability".to_owned()).is_err());
    assert!(app.error.is_some());
}

#[test]
fn the_regression_dialog_fits_predictors_and_confidence() {
    let mut app = FreePlotsApp::new();
    app.set_data(regression_frame(), "reg.csv".to_owned());

    app.open_regression_dialog();
    assert!(app.show_reg_dialog);
    app.reg_response = "y".to_owned();
    app.reg_predictors = vec!["x1".to_owned(), "x2".to_owned()];
    app.run_regression_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let text = app.session_text().to_owned();
    assert!(text.contains("Regression"), "{text}");
    assert!(text.contains("x1"), "{text}");

    app.session.clear();
    app.open_regression_dialog();
    app.reg_response = "y".to_owned();
    app.reg_predictors = vec!["x1".to_owned()];
    app.reg_confidence = 99.9;
    app.run_regression_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), text, "the confidence changed nothing");

    app.open_regression_dialog();
    app.reg_response.clear();
    app.run_regression_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the response column"));
    app.reg_response = "y".to_owned();
    app.reg_predictors.clear();
    app.run_regression_dialog();
    assert_eq!(app.error.as_deref(), Some("Select at least one predictor"));
}

#[test]
fn the_stepwise_dialog_honours_methods_and_criteria() {
    let mut app = FreePlotsApp::new();
    app.set_data(regression_frame(), "reg.csv".to_owned());

    let mut texts = Vec::new();
    for method in [
        StepMethod::Forward,
        StepMethod::Backward,
        StepMethod::Stepwise,
    ] {
        app.session.clear();
        app.open_stepwise_dialog();
        app.step_response = "y".to_owned();
        app.step_candidates = vec!["x1".to_owned(), "x2".to_owned(), "x3".to_owned()];
        app.step_method = method;
        app.run_stepwise_dialog();
        assert!(app.error.is_none(), "{method:?}: {:?}", app.error);
        texts.push(app.session_text().to_owned());
    }
    assert!(
        texts
            .iter()
            .all(|text| text.contains("Stepwise Regression"))
    );
    assert_ne!(texts[0], texts[1]);
    assert_ne!(texts[1], texts[2]);

    for criterion in [StepCriterion::Aic, StepCriterion::Bic] {
        app.session.clear();
        app.open_stepwise_dialog();
        app.step_response = "y".to_owned();
        app.step_candidates = vec!["x1".to_owned(), "x2".to_owned()];
        app.step_criterion = criterion;
        app.run_stepwise_dialog();
        assert!(app.error.is_none(), "{criterion:?}: {:?}", app.error);
    }

    app.open_stepwise_dialog();
    app.step_response.clear();
    app.run_stepwise_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the response column"));
    app.step_response = "y".to_owned();
    app.step_candidates.clear();
    app.run_stepwise_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select at least one candidate predictor")
    );
}

#[test]
fn every_one_way_kind_runs() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_way_frame(), "anova.csv".to_owned());
    for kind in [OneWayKind::Anova, OneWayKind::Levene, OneWayKind::Bartlett] {
        app.session.clear();
        app.open_one_way_dialog(kind);
        app.anova_response = "y".to_owned();
        app.anova_factor = "g".to_owned();
        app.run_anova_dialog();
        assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
        assert!(
            app.session_text().contains(kind.name()),
            "{} missing from the session",
            kind.name()
        );
    }

    app.open_one_way_dialog(OneWayKind::Anova);
    app.anova_response = "g".to_owned();
    app.anova_factor = "g".to_owned();
    app.run_anova_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The response and factor must be different")
    );
}

#[test]
fn the_two_way_dialog_validates_its_factors() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_way_frame(), "anova.csv".to_owned());
    app.open_two_way_dialog();
    app.two_way_response = "y".to_owned();
    app.two_way_first = "g".to_owned();
    app.two_way_second = "h".to_owned();
    app.run_two_way_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Two-Way"));

    app.open_two_way_dialog();
    app.two_way_response = "y".to_owned();
    app.two_way_first = "g".to_owned();
    app.two_way_second = "g".to_owned();
    app.run_two_way_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Select two different factor columns")
    );
}

#[test]
fn the_reliability_dialogs_fit_and_validate() {
    let mut app = FreePlotsApp::new();
    app.set_data(life_frame(), "life.csv".to_owned());

    app.open_weibull_dialog();
    app.weibull_time = "time".to_owned();
    app.run_weibull_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let plain = app.session_text().to_owned();
    assert!(plain.contains("Weibull"));

    app.session.clear();
    app.open_weibull_dialog();
    app.weibull_time = "time".to_owned();
    app.weibull_censor = Some("censor".to_owned());
    app.weibull_censor_value = "0".to_owned();
    app.run_weibull_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), plain, "censoring changed nothing");

    app.open_weibull_dialog();
    app.weibull_time = "time".to_owned();
    app.weibull_censor = Some("censor".to_owned());
    app.weibull_censor_value = "abc".to_owned();
    app.run_weibull_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Censoring value must be a number")
    );
    app.weibull_censor_value = "0".to_owned();
    app.weibull_time.clear();
    app.run_weibull_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the time column"));

    for (open, run, marker) in [
        (
            FreePlotsApp::open_exponential_dialog as fn(&mut FreePlotsApp),
            FreePlotsApp::run_exponential_dialog as fn(&mut FreePlotsApp),
            "Exponential",
        ),
        (
            FreePlotsApp::open_lognormal_dialog as fn(&mut FreePlotsApp),
            FreePlotsApp::run_lognormal_dialog as fn(&mut FreePlotsApp),
            "Lognormal",
        ),
    ] {
        app.session.clear();
        open(&mut app);
        if marker == "Exponential" {
            app.exp_time = "time".to_owned();
            app.exp_censor = Some("censor".to_owned());
            app.exp_censor_value = "0".to_owned();
        } else {
            app.log_time = "time".to_owned();
            app.log_censor = Some("censor".to_owned());
            app.log_censor_value = "0".to_owned();
        }
        run(&mut app);
        assert!(app.error.is_none(), "{marker}: {:?}", app.error);
        assert!(app.session_text().contains(marker), "{marker} missing");
    }
}

#[test]
fn the_alt_dialog_checks_the_horizon() {
    let mut app = FreePlotsApp::new();
    app.set_data(life_frame(), "life.csv".to_owned());
    app.open_alt_dialog();
    app.alt_time = "time".to_owned();
    app.alt_stress = "stress".to_owned();
    app.alt_use_stress = 80.0;
    app.alt_life = fp_core::alt::LifeModel::Weibull;
    app.alt_acceleration = fp_core::alt::AccelerationModel::Arrhenius;
    app.alt_horizon.clear();
    app.run_alt_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Accelerated"));

    app.session.clear();
    app.open_alt_dialog();
    app.alt_time = "time".to_owned();
    app.alt_stress = "stress".to_owned();
    app.alt_use_stress = 80.0;
    app.alt_horizon = "60".to_owned();
    app.run_alt_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);

    app.session.clear();
    app.open_alt_dialog();
    app.alt_time = "time".to_owned();
    app.alt_stress = "stress".to_owned();
    app.alt_use_stress = 80.0;
    app.alt_mode = "mode".to_owned();
    app.run_alt_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);

    app.open_alt_dialog();
    app.alt_time = "time".to_owned();
    app.alt_stress = "time".to_owned();
    app.run_alt_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The time and the stress must be different columns")
    );
    app.alt_stress = "stress".to_owned();
    app.alt_horizon = "abc".to_owned();
    app.run_alt_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The horizon must be a positive number")
    );
    app.alt_horizon = "-5".to_owned();
    app.run_alt_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The horizon must be a positive number")
    );
}

#[test]
fn the_kaplan_meier_confidence_changes_the_band() {
    let mut app = FreePlotsApp::new();
    app.set_data(life_frame(), "life.csv".to_owned());
    app.open_kaplan_meier_dialog();
    app.km_time = "time".to_owned();
    app.km_censor = Some("censor".to_owned());
    app.km_censor_value = "0".to_owned();
    app.km_confidence = 50.0;
    app.run_kaplan_meier_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let narrow = app.session_text().to_owned();
    assert!(narrow.contains("Kaplan"));

    app.session.clear();
    app.open_kaplan_meier_dialog();
    app.km_time = "time".to_owned();
    app.km_censor = Some("censor".to_owned());
    app.km_censor_value = "0".to_owned();
    app.km_confidence = 99.9;
    app.run_kaplan_meier_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), narrow, "the confidence changed nothing");

    app.open_kaplan_meier_dialog();
    app.km_time = "time".to_owned();
    app.km_censor = Some("censor".to_owned());
    app.km_censor_value = "x".to_owned();
    app.run_kaplan_meier_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Censoring value must be a number")
    );
}

#[test]
fn the_design_dialog_builds_factorials() {
    let mut app = FreePlotsApp::new();
    app.open_design_dialog();
    app.design_factors = "A,B".to_owned();
    app.design_replicates = 2;
    app.design_centers = 3;
    app.design_block_size = 0;
    app.design_randomize = false;
    app.run_design_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("design");
    assert_eq!(df.n_rows(), 11, "4 runs x 2 replicates + 3 centre points");
    let names: Vec<&str> = df.columns().iter().map(|column| column.name()).collect();
    assert!(names.contains(&"A") && names.contains(&"B"), "{names:?}");

    app.open_design_dialog();
    app.design_factors = "A,B".to_owned();
    app.design_replicates = 1;
    app.design_centers = 0;
    app.design_block_size = 2;
    app.run_design_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("design");
    assert_eq!(df.n_rows(), 4);
    assert!(df.columns().iter().any(|column| column.name() == "Block"));

    app.open_design_dialog();
    app.design_factors.clear();
    app.run_design_dialog();
    assert_eq!(app.error.as_deref(), Some("Enter at least one factor name"));
}

#[test]
fn the_box_behnken_and_plackett_burman_designs() {
    let mut app = FreePlotsApp::new();
    app.open_box_behnken_dialog();
    app.run_box_behnken_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("design");
    assert_eq!(
        df.columns()
            .iter()
            .filter(|column| matches!(column.name(), "A" | "B" | "C"))
            .count(),
        3
    );
    assert!(df.n_rows() >= 13, "{} runs", df.n_rows());

    app.open_box_behnken_dialog();
    app.design_factors = "A,B".to_owned();
    app.run_box_behnken_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("A Box-Behnken design needs at least three factors")
    );

    app.open_pb_dialog();
    app.design_runs = 8;
    app.design_randomize = false;
    app.run_pb_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.data().expect("design").n_rows(), 8);

    app.open_pb_dialog();
    app.design_runs = 7;
    app.run_pb_dialog();
    assert!(app.error.is_some(), "7 runs must be refused");
}

#[test]
fn the_factorial_analysis_reads_blocks_and_effects() {
    let mut app = FreePlotsApp::new();
    app.set_data(factorial_frame(), "doe.csv".to_owned());

    app.open_doe_dialog();
    app.doe_response = "y".to_owned();
    app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
    app.doe_block.clear();
    app.doe_main_effects = true;
    app.run_doe_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let plain = app.session_text().to_owned();
    assert!(plain.contains("Factorial"), "{plain}");

    app.session.clear();
    app.doe_block = "block".to_owned();
    app.run_doe_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), plain, "the block changed nothing");

    app.open_doe_dialog();
    app.doe_factors.clear();
    app.run_doe_dialog();
    assert_eq!(app.error.as_deref(), Some("Select at least one factor"));
}

#[test]
fn the_response_surface_flags_reach_the_engine() {
    let mut app = FreePlotsApp::new();
    app.set_data(desirability_frame(), "rsm.csv".to_owned());
    for (optimize, contours, steepest) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, true),
    ] {
        app.session.clear();
        app.open_rsm_dialog();
        app.rsm_response = "y".to_owned();
        app.rsm_factors = vec!["a".to_owned(), "b".to_owned()];
        app.rsm_optimize = optimize;
        app.rsm_contours = contours;
        app.rsm_steepest_ascent = steepest;
        app.run_rsm_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Response Surface"));
    }

    app.open_rsm_dialog();
    app.rsm_factors = vec!["a".to_owned()];
    app.run_rsm_dialog();
    assert_eq!(app.error.as_deref(), Some("Select at least two factors"));
}

#[test]
fn the_taguchi_design_and_analysis() {
    let mut app = FreePlotsApp::new();
    app.open_taguchi_design_dialog();
    app.taguchi_runs = 8;
    app.taguchi_design_factors = 3;
    app.design_randomize = false;
    app.run_taguchi_design_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let df = app.data().expect("design");
    assert_eq!(df.n_rows(), 8);
    assert_eq!(
        df.columns()
            .iter()
            .filter(|column| matches!(column.name(), "A" | "B" | "C"))
            .count(),
        3
    );

    app.set_data(taguchi_frame(), "taguchi.csv".to_owned());
    app.open_taguchi_dialog();
    app.taguchi_response = "y".to_owned();
    app.taguchi_factors = "A,B,C".to_owned();
    app.taguchi_quality = fp_core::taguchi::Quality::Smaller;
    app.run_taguchi_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let smaller = app.session_text().to_owned();
    assert!(smaller.contains("Taguchi"));

    app.session.clear();
    app.open_taguchi_dialog();
    app.taguchi_response = "y".to_owned();
    app.taguchi_factors = "A,B,C".to_owned();
    app.taguchi_quality = fp_core::taguchi::Quality::Larger;
    app.run_taguchi_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), smaller, "the quality changed nothing");

    app.open_taguchi_dialog();
    app.taguchi_response = "y".to_owned();
    app.taguchi_factors = "A,y".to_owned();
    app.run_taguchi_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The response cannot also be a factor")
    );
}

#[test]
fn the_gage_and_type_one_dialogs_run() {
    let mut app = FreePlotsApp::new();
    app.set_data(gage_frame(), "gage.csv".to_owned());
    app.open_gage_dialog();
    app.gage_measurement = "measure".to_owned();
    app.gage_part = "part".to_owned();
    app.gage_operator = "operator".to_owned();
    app.run_gage_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Gage R&R"));

    app.open_gage_dialog();
    app.gage_measurement = "part".to_owned();
    app.gage_part = "part".to_owned();
    app.gage_operator = "operator".to_owned();
    app.run_gage_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The three columns must be different")
    );

    app.open_type1_dialog();
    app.type1_measurement = "measure".to_owned();
    app.type1_reference = "10".to_owned();
    app.type1_lsl = "8".to_owned();
    app.type1_usl = "12".to_owned();
    app.run_type1_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Type 1"));

    app.type1_reference = "abc".to_owned();
    app.run_type1_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("Reference value must be a number")
    );
}

#[test]
fn the_linearity_and_agreement_dialogs_run() {
    let mut app = FreePlotsApp::new();
    app.set_data(linearity_frame(), "linearity.csv".to_owned());
    app.open_linearity_dialog();
    let names: Vec<String> = app
        .data()
        .expect("data")
        .columns()
        .iter()
        .map(|column| column.name().to_owned())
        .collect();
    assert!(names.contains(&"measure".to_owned()));
    app.linearity_measurement = "measure".to_owned();
    app.linearity_reference = "reference".to_owned();
    app.run_linearity_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Linearity"));

    app.open_linearity_dialog();
    app.linearity_measurement = "measure".to_owned();
    app.linearity_reference = "measure".to_owned();
    app.run_linearity_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The measurement and the reference must be different columns")
    );

    app.set_data(attribute_frame(), "agreement.csv".to_owned());
    app.open_agreement_dialog();
    app.agreement_part = "part".to_owned();
    app.agreement_appraiser = "appraiser".to_owned();
    app.agreement_rating = "rating".to_owned();
    app.run_agreement_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Attribute Agreement"));

    app.open_agreement_dialog();
    app.agreement_part = "part".to_owned();
    app.agreement_appraiser = "part".to_owned();
    app.agreement_rating = "rating".to_owned();
    app.run_agreement_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The three columns must be different")
    );
}

#[test]
fn the_multivariate_dialogs_take_their_options() {
    let mut app = FreePlotsApp::new();
    app.set_data(regression_frame(), "mv.csv".to_owned());

    app.open_pca_dialog();
    app.pca_columns = "x1,x2,x3".to_owned();
    app.pca_keep = 2;
    app.run_pca_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let two = app.session_text().to_owned();
    assert!(two.contains("Principal Component"));

    app.session.clear();
    app.open_pca_dialog();
    app.pca_columns = "x1,x2,x3".to_owned();
    app.pca_keep = 3;
    app.run_pca_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(
        app.session_text(),
        two,
        "the kept components changed nothing"
    );

    app.open_pca_dialog();
    app.pca_columns = "x1".to_owned();
    app.run_pca_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("List at least two columns, separated by commas")
    );

    app.session.clear();
    app.open_kmeans_dialog();
    app.kmeans_columns = "x1,x2".to_owned();
    app.kmeans_clusters = 2;
    app.kmeans_standardise = true;
    app.run_kmeans_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let two_clusters = app.session_text().to_owned();
    assert!(two_clusters.contains("K-Means"));

    app.session.clear();
    app.open_kmeans_dialog();
    app.kmeans_columns = "x1,x2".to_owned();
    app.kmeans_clusters = 3;
    app.kmeans_standardise = false;
    app.run_kmeans_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(
        app.session_text(),
        two_clusters,
        "the cluster count changed nothing"
    );

    app.session.clear();
    app.open_t2_dialog();
    app.t2_columns = "x1,x2".to_owned();
    app.t2_subgroup = 2;
    app.t2_confidence = 99.9;
    app.run_t2_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Hotelling"));
}

#[test]
fn the_time_series_dialogs_cover_their_options() {
    let mut app = FreePlotsApp::new();
    app.set_data(series_values_frame(), "series.csv".to_owned());

    app.open_timeseries_dialog();
    app.timeseries_column = "y".to_owned();
    app.timeseries_kind = TimeSeriesKind::MovingAverage;
    app.timeseries_window = 2;
    app.run_timeseries_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let small = app.session_text().to_owned();
    assert!(small.contains("Time Series"));

    app.session.clear();
    app.open_timeseries_dialog();
    app.timeseries_column = "y".to_owned();
    app.timeseries_kind = TimeSeriesKind::MovingAverage;
    app.timeseries_window = 5;
    app.run_timeseries_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), small, "the window changed nothing");

    app.session.clear();
    app.open_timeseries_dialog();
    app.timeseries_column = "y".to_owned();
    app.timeseries_kind = TimeSeriesKind::Decomposition;
    app.timeseries_period = 4;
    app.run_timeseries_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Time Series"));

    app.session.clear();
    app.open_timeseries_dialog();
    app.timeseries_column = "y".to_owned();
    app.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
    app.timeseries_season = 4;
    app.timeseries_horizon = 3;
    app.run_timeseries_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Time Series"));

    app.open_timeseries_dialog();
    app.timeseries_column.clear();
    app.run_timeseries_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the series column"));

    app.open_series_dialog();
    app.series_column = "y".to_owned();
    app.series_time = "y".to_owned();
    app.run_series_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The time column must be different from the series")
    );
}

#[test]
fn the_quality_tools_run_and_validate() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "quality.csv".to_owned());

    app.open_pareto_dialog();
    app.pareto_column = "before".to_owned();
    app.run_pareto_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Pareto"));

    app.open_stem_dialog();
    app.stem_column = "before".to_owned();
    app.run_stem_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Stem"));

    app.open_ecdf_dialog();
    app.ecdf_column = "before".to_owned();
    app.ecdf_confidence = 50.0;
    app.run_ecdf_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let narrow = app.session_text().to_owned();
    app.session.clear();
    app.open_ecdf_dialog();
    app.ecdf_column = "before".to_owned();
    app.ecdf_confidence = 99.9;
    app.run_ecdf_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), narrow);

    app.open_dotplot_dialog();
    app.dot_column = "before".to_owned();
    app.run_dotplot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    app.dot_factor = "before".to_owned();
    app.run_dotplot_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The factor must be a different column")
    );

    app.open_matrix_dialog();
    app.matrix_columns = "before,after".to_owned();
    app.matrix_bins = 5;
    app.run_matrix_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    app.matrix_columns = "before".to_owned();
    app.run_matrix_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("List at least two columns, separated by commas")
    );

    app.open_interval_dialog();
    app.interval_response = "before".to_owned();
    app.interval_factor = "before".to_owned();
    app.run_interval_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The response and the factor must be different columns")
    );

    app.session.clear();
    app.set_data(two_way_frame(), "interval.csv".to_owned());
    app.open_interval_dialog();
    app.interval_response = "y".to_owned();
    app.interval_factor = "g".to_owned();
    app.interval_confidence = 99.9;
    app.run_interval_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Interval Plot"));

    app.session.clear();
    app.set_data(two_sample_frame(), "quality.csv".to_owned());
    app.run_run_chart("before");
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Run Chart"));
    assert!(app.plot().is_some());
}

#[test]
fn the_remaining_option_dialogs_run() {
    let mut app = FreePlotsApp::new();
    app.set_data(regression_frame(), "options.csv".to_owned());

    // Attribute charts: constant size for C, subgroup size for U.
    for (kind, size) in [
        (AttributeChartKind::C, "5"),
        (AttributeChartKind::U, "5"),
        (AttributeChartKind::LaneyP, "5"),
    ] {
        app.session.clear();
        app.attr_column = "x2".to_owned();
        app.attr_kind = kind;
        app.attr_size = size.to_owned();
        app.attr_sizes = None;
        app.run_attribute_chart();
        assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
        assert!(app.session_text().contains(kind.name()));
    }
    app.attr_kind = AttributeChartKind::U;
    app.attr_size = "0".to_owned();
    app.run_attribute_chart();
    assert_eq!(
        app.error.as_deref(),
        Some("Constant subgroup size must be a positive integer")
    );

    // G chart with and without a times column.
    app.session.clear();
    app.open_g_chart_dialog();
    app.g_chart_counts = "x3".to_owned();
    app.g_chart_times.clear();
    app.run_g_chart_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let plain = app.session_text().to_owned();
    assert!(plain.contains("G Chart"));

    app.session.clear();
    app.open_g_chart_dialog();
    app.g_chart_counts = "x3".to_owned();
    app.g_chart_times = "x1".to_owned();
    app.run_g_chart_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), plain, "the times changed nothing");

    // Sampling: a plan and a plan evaluation.
    app.session.clear();
    app.sampling_plan_mode = true;
    app.sampling_aql = 1.0;
    app.sampling_ltpd = 5.0;
    app.sampling_alpha = 5.0;
    app.sampling_beta = 10.0;
    app.sampling_lot = 1000;
    app.run_sampling_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Acceptance"));

    app.session.clear();
    app.sampling_plan_mode = false;
    app.sampling_lot = 1000;
    app.sampling_sample = 20;
    app.sampling_accept = 1;
    app.run_sampling_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);

    // Box-Cox, tolerance interval, partial correlation and the curve fit.
    app.session.clear();
    app.open_box_cox_dialog();
    app.box_cox_column = "y".to_owned();
    app.run_box_cox_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Box-Cox"));

    app.session.clear();
    app.open_tolerance_dialog();
    app.tolerance_column = "y".to_owned();
    app.tolerance_proportion = 99.0;
    app.tolerance_confidence = 99.0;
    app.tolerance_free = false;
    app.run_tolerance_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Tolerance"));

    // The distribution-free interval needs a far larger sample.
    app.session.clear();
    app.open_tolerance_dialog();
    app.tolerance_column = "y".to_owned();
    app.tolerance_proportion = 99.0;
    app.tolerance_confidence = 99.0;
    app.tolerance_free = true;
    app.run_tolerance_dialog();
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("distribution-free")),
        "{:?}",
        app.error
    );

    app.session.clear();
    app.open_partial_dialog();
    app.partial_x = "x1".to_owned();
    app.partial_y = "y".to_owned();
    app.partial_control = "x2".to_owned();
    app.run_partial_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let partial = app.session_text().to_owned();
    assert!(partial.contains("Partial"), "{partial}");

    app.open_partial_dialog();
    app.partial_x = "x1".to_owned();
    app.partial_y = "x1".to_owned();
    app.partial_control = "x2".to_owned();
    app.run_partial_dialog();
    assert_eq!(app.error.as_deref(), Some("Select three different columns"));

    app.session.clear();
    app.open_correlation_dialog();
    app.correlation_columns = "x1,x2,x3".to_owned();
    app.run_correlation_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Correlation"));

    for model in [
        CurveModel::Polynomial(1),
        CurveModel::Polynomial(2),
        CurveModel::Exponential,
    ] {
        app.session.clear();
        app.open_curve_dialog();
        app.curve_x = "x1".to_owned();
        app.curve_y = "y".to_owned();
        app.curve_model = model;
        app.run_curve_dialog();
        assert!(app.error.is_none(), "{model:?}: {:?}", app.error);
        assert!(app.session_text().contains("Curve Fit"));
    }
    app.open_curve_dialog();
    app.curve_x = "x1".to_owned();
    app.curve_y = "x1".to_owned();
    app.run_curve_dialog();
    assert_eq!(app.error.as_deref(), Some("Select two different columns"));
}

#[test]
fn the_probability_plot_dialog_honours_its_options() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_sample_frame(), "pp.csv".to_owned());

    app.open_probability_plot_dialog();
    app.dialog_columns = vec!["before".to_owned()];
    app.dialog_distribution = DistributionKind::Normal;
    app.dialog_display = PlotDisplay::Both;
    app.dialog_ci = true;
    app.dialog_ci_level = 95.0;
    app.run_probability_plot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.session_text().contains("Probability Plot"));
    assert!(app.plot().is_some());
    let single = app.session_text().to_owned();

    app.session.clear();
    app.open_probability_plot_dialog();
    app.dialog_columns = vec!["before".to_owned()];
    app.dialog_ci_level = 99.9;
    app.run_probability_plot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(app.session_text(), single, "the level changed nothing");

    // Two columns: the layout and the shared scales shape the report.
    app.session.clear();
    app.open_probability_plot_dialog();
    app.dialog_columns = vec!["before".to_owned(), "after".to_owned()];
    app.dialog_layout = PlotLayout::Overlay;
    app.dialog_same_x = true;
    app.dialog_same_y = true;
    app.run_probability_plot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let overlay = app.session_text().to_owned();
    assert!(overlay.contains("Overlay"), "{overlay}");
    assert!(overlay.contains("same X/Y"), "{overlay}");

    app.session.clear();
    app.open_probability_plot_dialog();
    app.dialog_columns = vec!["before".to_owned(), "after".to_owned()];
    app.dialog_layout = PlotLayout::Separate;
    app.run_probability_plot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let separate = app.session_text().to_owned();
    assert!(separate.contains("Separate"), "{separate}");
    assert_ne!(overlay, separate);

    // A distribution without a band, and symbols-only, both suppress the CI.
    for (distribution, display) in [
        (DistributionKind::Weibull, PlotDisplay::Both),
        (DistributionKind::Normal, PlotDisplay::Symbols),
    ] {
        app.session.clear();
        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["before".to_owned()];
        app.dialog_distribution = distribution;
        app.dialog_display = display;
        app.dialog_ci = true;
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{distribution:?}: {:?}", app.error);
        assert!(!app.session_text().is_empty());
    }

    // A grouping column is accepted for a single Y variable.
    app.set_data(two_way_frame(), "groups.csv".to_owned());
    app.session.clear();
    app.open_probability_plot_dialog();
    app.dialog_columns = vec!["y".to_owned()];
    app.dialog_group = Some("g".to_owned());
    app.run_probability_plot_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        app.session_text().contains("grouped"),
        "{}",
        app.session_text()
    );

    app.open_probability_plot_dialog();
    app.dialog_columns.clear();
    app.run_probability_plot_dialog();
    assert_eq!(app.error.as_deref(), Some("Select at least one Y variable"));
}

#[test]
fn the_grouped_descriptives_dialog_runs_per_level() {
    let mut app = FreePlotsApp::new();
    app.set_data(two_way_frame(), "groups.csv".to_owned());
    app.open_describe_by_dialog();
    app.describe_column = "y".to_owned();
    app.describe_by = Some("g".to_owned());
    app.run_describe_by_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let text = app.session_text().to_owned();
    for level in ["A", "B", "C"] {
        assert!(text.contains(&format!("g = {level}")), "{text}");
    }
    assert!(!app.show_describe_dialog);

    app.open_describe_by_dialog();
    app.describe_column.clear();
    app.run_describe_by_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the numeric column"));
    app.describe_column = "y".to_owned();
    app.describe_by = None;
    app.run_describe_by_dialog();
    assert_eq!(app.error.as_deref(), Some("Select the grouping column"));
    app.describe_by = Some("y".to_owned());
    app.run_describe_by_dialog();
    assert_eq!(
        app.error.as_deref(),
        Some("The grouping column must be different")
    );
}

#[test]
fn every_dialog_renders_without_panicking() {
    use egui_kittest::Harness;

    type Opener = fn(&mut FreePlotsApp);

    fn openers() -> Vec<(&'static str, Opener)> {
        vec![
            (
                "open_probability_plot_dialog",
                FreePlotsApp::open_probability_plot_dialog,
            ),
            ("open_attribute_dialog", FreePlotsApp::open_attribute_dialog),
            (
                "open_correlation_dialog",
                FreePlotsApp::open_correlation_dialog,
            ),
            ("open_curve_dialog", FreePlotsApp::open_curve_dialog),
            ("open_two_way_dialog", FreePlotsApp::open_two_way_dialog),
            ("open_crosstab_dialog", FreePlotsApp::open_crosstab_dialog),
            ("open_g_chart_dialog", FreePlotsApp::open_g_chart_dialog),
            ("open_partial_dialog", FreePlotsApp::open_partial_dialog),
            ("open_tolerance_dialog", FreePlotsApp::open_tolerance_dialog),
            ("open_agreement_dialog", FreePlotsApp::open_agreement_dialog),
            ("open_taguchi_dialog", FreePlotsApp::open_taguchi_dialog),
            (
                "open_taguchi_design_dialog",
                FreePlotsApp::open_taguchi_design_dialog,
            ),
            ("open_sampling_dialog", FreePlotsApp::open_sampling_dialog),
            ("open_t2_dialog", FreePlotsApp::open_t2_dialog),
            ("open_kmeans_dialog", FreePlotsApp::open_kmeans_dialog),
            ("open_pca_dialog", FreePlotsApp::open_pca_dialog),
            ("open_sort_dialog", FreePlotsApp::open_sort_dialog),
            ("open_stack_dialog", FreePlotsApp::open_stack_dialog),
            ("open_unstack_dialog", FreePlotsApp::open_unstack_dialog),
            ("open_date_diff_dialog", FreePlotsApp::open_date_diff_dialog),
            (
                "open_date_shift_dialog",
                FreePlotsApp::open_date_shift_dialog,
            ),
            ("open_subset_dialog", FreePlotsApp::open_subset_dialog),
            (
                "open_timeseries_dialog",
                FreePlotsApp::open_timeseries_dialog,
            ),
            ("open_alt_dialog", FreePlotsApp::open_alt_dialog),
            (
                "open_step_stress_dialog",
                FreePlotsApp::open_step_stress_dialog,
            ),
            ("open_series_dialog", FreePlotsApp::open_series_dialog),
            ("open_stem_dialog", FreePlotsApp::open_stem_dialog),
            ("open_ecdf_dialog", FreePlotsApp::open_ecdf_dialog),
            ("open_dotplot_dialog", FreePlotsApp::open_dotplot_dialog),
            ("open_matrix_dialog", FreePlotsApp::open_matrix_dialog),
            ("open_interval_dialog", FreePlotsApp::open_interval_dialog),
            ("open_calc_dialog", FreePlotsApp::open_calc_dialog),
            ("open_pareto_dialog", FreePlotsApp::open_pareto_dialog),
            ("open_ordinal_dialog", FreePlotsApp::open_ordinal_dialog),
            ("open_poisson_dialog", FreePlotsApp::open_poisson_dialog),
            ("open_logistic_dialog", FreePlotsApp::open_logistic_dialog),
            ("open_linearity_dialog", FreePlotsApp::open_linearity_dialog),
            ("open_box_cox_dialog", FreePlotsApp::open_box_cox_dialog),
            ("open_power_dialog", FreePlotsApp::open_power_dialog),
            (
                "open_regression_dialog",
                FreePlotsApp::open_regression_dialog,
            ),
            ("open_stepwise_dialog", FreePlotsApp::open_stepwise_dialog),
            ("open_anova_dialog", FreePlotsApp::open_anova_dialog),
            (
                "open_kaplan_meier_dialog",
                FreePlotsApp::open_kaplan_meier_dialog,
            ),
            ("open_gage_dialog", FreePlotsApp::open_gage_dialog),
            ("open_doe_dialog", FreePlotsApp::open_doe_dialog),
            ("open_design_dialog", FreePlotsApp::open_design_dialog),
            (
                "open_box_behnken_dialog",
                FreePlotsApp::open_box_behnken_dialog,
            ),
            ("open_pb_dialog", FreePlotsApp::open_pb_dialog),
            ("open_rsm_dialog", FreePlotsApp::open_rsm_dialog),
            (
                "open_desirability_dialog",
                FreePlotsApp::open_desirability_dialog,
            ),
            ("open_scatter_dialog", FreePlotsApp::open_scatter_dialog),
            ("open_weibull_dialog", FreePlotsApp::open_weibull_dialog),
            ("open_type1_dialog", FreePlotsApp::open_type1_dialog),
            (
                "open_exponential_dialog",
                FreePlotsApp::open_exponential_dialog,
            ),
            ("open_lognormal_dialog", FreePlotsApp::open_lognormal_dialog),
            ("open_find_dialog", FreePlotsApp::open_find_dialog),
            ("open_format_dialog", FreePlotsApp::open_format_dialog),
        ]
    }

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 700.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.step();
    for (name, open) in openers() {
        open(harness.state_mut());
        harness.step();
        harness.state_mut().close_dialogs();
        harness.step();
        assert!(
            harness.state().session_text().len() < 1_000_000,
            "dialog {name} produced too much session text"
        );
    }
}

#[test]
fn arrows_tab_and_enter_move_the_selection() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 1)));
    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 2)));
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selection_size(), (1, 2));
    harness.key_press(egui::Key::ArrowRight);
    harness.run();
    assert_eq!(
        harness.state().selected_cell().map(|(column, _)| column),
        Some(1)
    );
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((1, 4)));
}

#[test]
fn the_arrows_reach_the_spare_columns() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let columns = app.data().expect("data").n_columns();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    harness.state_mut().select_cell(columns - 1, 0);
    harness.run();
    harness.key_press(egui::Key::ArrowRight);
    harness.run();
    // Right from the last real column reaches C9, which becomes real.
    assert_eq!(
        harness.state().selected_cell().map(|(c, _)| c),
        Some(columns)
    );
    assert_eq!(
        harness.state().data().expect("data").n_columns(),
        columns + 1
    );
    assert_eq!(
        harness.state().selected.as_deref(),
        Some(format!("C{}", columns + 1).as_str())
    );
    // Writing into it works like any other cell.
    harness
        .state_mut()
        .begin_inline_edit_with(columns, 0, "3.5");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness
            .state()
            .data()
            .and_then(|df| df.cell_text(columns, 0)),
        Some("3.5".to_owned())
    );
    // Left comes back without removing the column.
    harness.key_press(egui::Key::ArrowLeft);
    harness.run();
    assert_eq!(
        harness.state().selected_cell().map(|(c, _)| c),
        Some(columns - 1)
    );
    assert_eq!(
        harness.state().data().expect("data").n_columns(),
        columns + 1
    );
}

#[test]
fn the_cell_editor_commits_and_moves_with_the_arrows() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // Typing in a cell and pressing Down commits the value and selects the
    // cell below, without going back to the mouse.
    harness.state_mut().select_cell(0, 0);
    harness.state_mut().begin_inline_edit_with(0, 0, "42");
    harness.run();
    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 1)));
    assert_eq!(
        harness.state().data().and_then(|df| df.cell_text(0, 0)),
        Some("42".to_owned())
    );
    assert!(harness.state().inline_edit.is_none(), "the editor closes");

    // Right and Left move by one column and commit too.
    harness.state_mut().begin_inline_edit_with(0, 1, "7");
    harness.run();
    harness.key_press(egui::Key::ArrowRight);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((1, 1)));
    assert_eq!(
        harness.state().data().and_then(|df| df.cell_text(0, 1)),
        Some("7".to_owned())
    );

    // Tab goes right, Shift+Tab comes back.
    harness.state_mut().begin_inline_edit_with(0, 2, "9");
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((1, 2)));
    harness.state_mut().begin_inline_edit_with(1, 2, "10");
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 2)));

    // Escape still cancels without writing.
    harness.state_mut().select_cell(0, 3);
    let before = harness
        .state()
        .data()
        .and_then(|df| df.cell_text(0, 3))
        .unwrap_or_default();
    harness.state_mut().begin_inline_edit_with(0, 3, "999");
    harness.run();
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 3)));
    assert_eq!(
        harness
            .state()
            .data()
            .and_then(|df| df.cell_text(0, 3))
            .unwrap_or_default(),
        before
    );
}

#[test]
fn cells_past_the_eighth_column_can_be_written() {
    let mut app = FreePlotsApp::new();
    assert_eq!(app.data().map(DataFrame::n_columns), Some(8));
    // The grid shows 26 columns; the ones past the data become real when used.
    app.ensure_columns(9);
    assert_eq!(app.data().map(DataFrame::n_columns), Some(9));
    app.select_cell(8, 0);
    assert_eq!(app.selected_column(), Some("C9"));
    app.begin_inline_edit_with(8, 0, "5");
    assert!(app.commit_inline_edit());
    assert_eq!(
        app.data().and_then(|df| df.cell_text(8, 0)),
        Some("5".to_owned())
    );
    // Asking for an earlier width changes nothing.
    app.ensure_columns(3);
    assert_eq!(app.data().map(DataFrame::n_columns), Some(9));
}

#[test]
fn the_headers_select_the_column_the_row_and_the_sheet() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    harness.get_by_label("C2").click();
    harness.run();
    assert_eq!(harness.state().selected_column(), Some("batch"));

    harness.hover_at(egui::pos2(20.0, 210.0));
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(20.0, 210.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    assert_eq!(harness.state().selection_size().1, 1);
    assert_eq!(harness.state().selected_cell().map(|(_, row)| row), Some(2));
}

#[test]
fn the_cell_context_menu_inserts_a_row_and_undo_removes_it() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.hover_at(egui::pos2(90.0, 170.0));
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(90.0, 170.0),
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    harness.get_by_label("Insert row below").click();
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 6);
    harness.state_mut().undo();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 5);
}

fn open_cell_menu(harness: &mut egui_kittest::Harness<'_, FreePlotsApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
}

#[test]
fn every_cell_context_menu_action_works_and_undoes() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // Insert a row above the second row.
    harness.state_mut().select_cell(0, 1);
    open_cell_menu(&mut harness, egui::pos2(90.0, 190.0));
    harness.get_by_label("Insert row above").click();
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 6);
    harness.state_mut().undo();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 5);

    // Delete the selected row.
    harness.state_mut().select_cell(0, 1);
    open_cell_menu(&mut harness, egui::pos2(90.0, 190.0));
    harness.get_by_label("Delete row").click();
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 4);
    harness.state_mut().undo();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 5);

    // Add a numeric column, then a text column, both empty.
    harness.state_mut().select_cell(0, 0);
    open_cell_menu(&mut harness, egui::pos2(90.0, 170.0));
    harness.get_by_label("Add numeric column").click();
    harness.run();
    let columns = harness.state().data().expect("frame").n_columns();
    assert_eq!(columns, 3);
    harness.state_mut().undo();

    // Delete a column.
    harness.state_mut().select_cell(1, 0);
    open_cell_menu(&mut harness, egui::pos2(150.0, 170.0));
    harness.get_by_label("Delete column").click();
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_columns(), 1);
    harness.state_mut().undo();
    assert_eq!(harness.state().data().expect("frame").n_columns(), 2);

    // Rename through the dialog the menu opens.
    harness.state_mut().select_cell(1, 0);
    open_cell_menu(&mut harness, egui::pos2(150.0, 170.0));
    harness.get_by_label("Rename column…").click();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "New name:")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "New name:")
        .type_text("weight");
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    assert!(
        harness
            .state()
            .data()
            .expect("frame")
            .column("weight")
            .is_ok(),
        "the column was not renamed"
    );
    harness.state_mut().undo();
    assert!(
        harness
            .state()
            .data()
            .expect("frame")
            .column("batch")
            .is_ok(),
        "undo restores the old name"
    );
}

#[test]
fn a_ten_thousand_character_cell_survives_the_round_trip() {
    let long = "A".repeat(10_000);
    let values = vec![long.clone(), "short".to_owned()];
    let df = DataFrame::new(vec![Column::new_text("text", values)]).expect("frame");
    let app_json = fp_core::io::to_project_json(&df, "long.fpp").expect("project json");
    let back = fp_core::io::from_project_json(&app_json).expect("project read");
    assert_eq!(back.n_rows(), 2);
    assert_eq!(back.cell_text(0, 0).as_deref().map(str::len), Some(10_000));

    let bytes = fp_core::xlsx::workbook(&df);
    let back = fp_core::xlsx_read::read_workbook_sheet(&bytes, None).expect("xlsx read");
    assert_eq!(back.cell_text(0, 0).as_deref().map(str::len), Some(10_000));

    let mut app = FreePlotsApp::new();
    app.set_data(df, "long.csv".to_owned());
    // The editor keeps the whole value, never a truncated copy.
    app.select_cell(0, 0);
    let shown = app
        .data()
        .and_then(|frame| frame.cell_text(0, 0))
        .unwrap_or_default();
    assert_eq!(shown.len(), 10_000);
}

#[test]
fn the_undo_stack_stays_bounded_under_large_edits() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let big = "x".repeat(200_000);
    for step in 0..120 {
        app.select_cell(1, 0);
        app.set_edit_buffer(format!("{big}{step}"));
        assert!(app.commit_cell(), "edit {step} refused: {:?}", app.error);
    }
    assert!(
        app.undo_stack.len() <= 100,
        "undo stack kept {} groups",
        app.undo_stack.len()
    );
    // The newest edit is undoable and the text is restored to a previous
    // large value rather than to the original cell.
    app.undo();
    let text = app
        .data()
        .expect("frame")
        .cell_text(1, 0)
        .unwrap_or_default();
    assert!(text.len() > 100_000, "undo left {} characters", text.len());
}

#[test]
fn the_stepwise_and_test_option_choices_persist() {
    use fp_core::stepwise::{StepCriterion, StepMethod};
    use fp_core::variance::VarianceCenter;

    let dir = std::env::temp_dir().join(format!("fp-options-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.step_method = StepMethod::Backward;
    app.step_criterion = StepCriterion::Aic;
    app.variance_center = VarianceCenter::Median;
    app.two_way_interaction = true;
    app.test2_success = "0".to_owned();
    app.equivalence_null = "2.5".to_owned();
    app.equivalence_margin = "0.25".to_owned();
    app.persist_preferences();

    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.step_method, StepMethod::Backward);
    assert_eq!(restored.step_criterion, StepCriterion::Aic);
    assert_eq!(restored.variance_center, VarianceCenter::Median);
    assert!(restored.two_way_interaction);
    assert_eq!(restored.test2_success, "0");
    assert_eq!(restored.equivalence_null, "2.5");
    assert_eq!(restored.equivalence_margin, "0.25");

    // Opening the dialogs keeps the stored choices.
    restored.set_data(frame(), "weights.csv".to_owned());
    restored.open_stepwise_dialog();
    restored.open_one_way_dialog(OneWayKind::Levene);
    restored.open_equivalence_dialog(EquivalenceKind::OneSample);
    restored.open_test2_dialog(Test2Kind::OneProportion);
    assert_eq!(restored.step_method, StepMethod::Backward);
    assert_eq!(restored.step_criterion, StepCriterion::Aic);
    assert_eq!(restored.variance_center, VarianceCenter::Median);
    assert_eq!(restored.test2_success, "0");
    assert_eq!(restored.equivalence_null, "2.5");
    assert_eq!(restored.equivalence_margin, "0.25");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn graph_edits_do_not_leak_into_another_graph() {
    use fp_core::plot::{Annotation, ReferenceAxis, ReferenceLine};
    use fp_core::workflow::AnalysisSpec;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("probability plot");
    app.graph_overlay.subtitle = Some("Refe".to_owned());
    app.graph_overlay.footer = Some("edgedgsg".to_owned());
    app.graph_overlay.reference_lines.push(ReferenceLine {
        axis: ReferenceAxis::Vertical,
        value: 12.0,
        label: Some("verdhdfgfdg".to_owned()),
    });
    app.graph_overlay.annotations.push(Annotation {
        text: "note".to_owned(),
        ..Annotation::default()
    });
    app.graph_overlay.x_range = Some((11.0, 14.0));

    // A different graph starts clean: no letters or lines from the old one.
    app.run_spec_blocking(AnalysisSpec::histogram("weight"), "Histogram".to_owned())
        .expect("histogram");
    assert!(app.graph_overlay.reference_lines.is_empty());
    assert!(app.graph_overlay.annotations.is_empty());
    assert!(app.graph_overlay.subtitle.is_none());
    assert!(app.graph_overlay.footer.is_none());
    assert!(app.graph_overlay.x_range.is_none());

    // Running the same graph again keeps the edits made for it.
    app.run_spec_blocking(
        AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("probability plot");
    app.graph_overlay.annotations.push(Annotation {
        text: "keep".to_owned(),
        ..Annotation::default()
    });
    app.run_spec_blocking(
        AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("probability plot");
    assert_eq!(app.graph_overlay.annotations.len(), 1);
    assert_eq!(app.graph_overlay.annotations[0].text, "keep");
}

#[test]
fn the_analysis_method_choices_persist() {
    use fp_core::attribute::AttributeChartKind;
    use fp_core::curvefit::CurveModel;
    use fp_core::hypothesis::Alternative;
    use fp_core::power::PowerProcedure;
    use fp_core::proportion::ProportionMethod;
    use fp_core::variance::VarianceMethod;

    let dir = std::env::temp_dir().join(format!("fp-method-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.cap_subgroup = 4;
    app.cap_bootstrap = 500;
    app.cap_box_cox = true;
    app.cap_require_stability = true;
    app.t_alternative = Alternative::Less;
    app.t_pooled = true;
    app.test2_method = ProportionMethod::Normal;
    app.test2_variance_method = VarianceMethod::Bonett;
    app.test2_alternative = Alternative::Greater;
    app.rank_alternative = Alternative::Less;
    app.power_procedure = PowerProcedure::TwoSampleT;
    app.power_alpha = 10.0;
    app.power_target = 90.0;
    app.power_alternative = Alternative::Greater;
    app.power_groups = 4;
    app.curve_model = CurveModel::Exponential;
    app.scatter_fit = false;
    app.rsm_optimize = true;
    app.rsm_contours = true;
    app.rsm_steepest_ascent = true;
    app.doe_main_effects = true;
    app.kmeans_clusters = 5;
    app.kmeans_standardise = false;
    app.kmeans_seed = 42;
    app.attr_kind = AttributeChartKind::LaneyP;
    app.attr_size = "250".to_owned();
    app.t2_subgroup = 7;
    app.tolerance_proportion = 99.0;
    app.tolerance_free = true;
    app.diagnostic_alpha = 1.0;
    app.persist_preferences();

    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.cap_subgroup, 4);
    assert_eq!(restored.cap_bootstrap, 500);
    assert!(restored.cap_box_cox);
    assert!(restored.cap_require_stability);
    assert_eq!(restored.t_alternative, Alternative::Less);
    assert!(restored.t_pooled);
    assert_eq!(restored.test2_method, ProportionMethod::Normal);
    assert_eq!(restored.test2_variance_method, VarianceMethod::Bonett);
    assert_eq!(restored.test2_alternative, Alternative::Greater);
    assert_eq!(restored.rank_alternative, Alternative::Less);
    assert_eq!(restored.power_procedure, PowerProcedure::TwoSampleT);
    assert!((restored.power_alpha - 10.0).abs() < 1e-9);
    assert!((restored.power_target - 90.0).abs() < 1e-9);
    assert_eq!(restored.power_alternative, Alternative::Greater);
    assert_eq!(restored.power_groups, 4);
    assert_eq!(restored.curve_model, CurveModel::Exponential);
    assert!(!restored.scatter_fit);
    assert!(restored.rsm_optimize);
    assert!(restored.rsm_contours);
    assert!(restored.rsm_steepest_ascent);
    assert!(restored.doe_main_effects);
    assert_eq!(restored.kmeans_clusters, 5);
    assert!(!restored.kmeans_standardise);
    assert_eq!(restored.kmeans_seed, 42);
    assert_eq!(restored.attr_kind, AttributeChartKind::LaneyP);
    assert_eq!(restored.attr_size, "250");
    assert_eq!(restored.t2_subgroup, 7);
    assert!((restored.tolerance_proportion - 99.0).abs() < 1e-9);
    assert!(restored.tolerance_free);
    assert!((restored.diagnostic_alpha - 1.0).abs() < 1e-9);

    // Opening the dialogs keeps the stored method choices.
    restored.set_data(frame(), "weights.csv".to_owned());
    restored.open_probability_plot_dialog();
    restored.open_t_test_dialog(TTestKind::OneSample);
    restored.open_test2_dialog(Test2Kind::TwoProportions);
    restored.open_power_dialog();
    restored.open_curve_dialog();
    restored.open_kmeans_dialog();
    restored.open_t2_dialog();
    restored.open_tolerance_dialog();
    assert!(restored.cap_box_cox);
    assert!(restored.cap_require_stability);
    assert_eq!(restored.t_alternative, Alternative::Less);
    assert!(restored.t_pooled);
    assert_eq!(restored.test2_variance_method, VarianceMethod::Bonett);
    assert_eq!(restored.power_procedure, PowerProcedure::TwoSampleT);
    assert!((restored.power_target - 90.0).abs() < 1e-9);
    assert_eq!(restored.curve_model, CurveModel::Exponential);
    assert_eq!(restored.kmeans_clusters, 5);
    assert_eq!(restored.attr_kind, AttributeChartKind::LaneyP);
    assert_eq!(restored.t2_subgroup, 7);
    assert!((restored.tolerance_proportion - 99.0).abs() < 1e-9);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_doe_pca_timeseries_and_stepwise_choices_persist() {
    use fp_core::timeseries::DecompositionModel;

    let dir = std::env::temp_dir().join(format!("fp-design-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.design_factors = "X,Y,Z".to_owned();
    app.design_runs = 20;
    app.design_replicates = 3;
    app.design_centers = 4;
    app.design_block_size = 2;
    app.design_randomize = false;
    app.design_seed = 777;
    app.taguchi_runs = 16;
    app.taguchi_design_factors = 5;
    app.pca_keep = 4;
    app.matrix_bins = 12;
    app.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
    app.timeseries_window = 5;
    app.timeseries_period = 6;
    app.timeseries_model = DecompositionModel::Multiplicative;
    app.timeseries_season = 6;
    app.timeseries_horizon = 8;
    app.step_max_steps = 25;
    app.step_alpha_enter = 0.05;
    app.step_alpha_remove = 0.10;
    app.persist_preferences();

    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.design_factors, "X,Y,Z");
    assert_eq!(restored.design_runs, 20);
    assert_eq!(restored.design_replicates, 3);
    assert_eq!(restored.design_centers, 4);
    assert_eq!(restored.design_block_size, 2);
    assert!(!restored.design_randomize);
    assert_eq!(restored.design_seed, 777);
    assert_eq!(restored.taguchi_runs, 16);
    assert_eq!(restored.taguchi_design_factors, 5);
    assert_eq!(restored.pca_keep, 4);
    assert_eq!(restored.matrix_bins, 12);
    assert_eq!(
        restored.timeseries_kind,
        TimeSeriesKind::ExponentialSmoothing
    );
    assert_eq!(restored.timeseries_window, 5);
    assert_eq!(restored.timeseries_period, 6);
    assert_eq!(
        restored.timeseries_model,
        DecompositionModel::Multiplicative
    );
    assert_eq!(restored.timeseries_season, 6);
    assert_eq!(restored.timeseries_horizon, 8);
    assert_eq!(restored.step_max_steps, 25);
    assert!((restored.step_alpha_enter - 0.05).abs() < 1e-9);
    assert!((restored.step_alpha_remove - 0.10).abs() < 1e-9);

    // Opening the dialogs keeps the stored choices instead of resetting them.
    restored.open_design_dialog();
    restored.open_stepwise_dialog();
    restored.open_pca_dialog();
    restored.open_matrix_dialog();
    restored.open_taguchi_design_dialog();
    assert_eq!(restored.design_factors, "X,Y,Z");
    assert_eq!(restored.design_replicates, 3);
    assert_eq!(restored.pca_keep, 4);
    assert_eq!(restored.matrix_bins, 12);
    assert_eq!(restored.taguchi_runs, 16);
    assert!((restored.step_alpha_enter - 0.05).abs() < 1e-9);

    // A two-factor factorial still opens Box–Behnken with three factors.
    restored.design_factors = "A,B".to_owned();
    restored.open_box_behnken_dialog();
    assert_eq!(restored.design_factors, "A,B,C");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_gage_reliability_and_sampling_choices_persist() {
    use fp_core::alt::{AccelerationModel, LifeModel};
    use fp_core::gage::GageDesign;

    let dir = std::env::temp_dir().join(format!("fp-domain-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.gage_design = GageDesign::Nested;
    app.alt_life = LifeModel::Lognormal;
    app.alt_acceleration = AccelerationModel::InversePower;
    app.alt_use_stress = 350.0;
    app.ss_life = LifeModel::Exponential;
    app.ss_acceleration = AccelerationModel::Eyring;
    app.ss_use_stress = 320.0;
    app.sampling_sample = 80;
    app.sampling_accept = 2;
    app.sampling_lot = 5000;
    app.sampling_aql = 1.5;
    app.sampling_ltpd = 4.0;
    app.sampling_alpha = 5.0;
    app.sampling_beta = 10.0;
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.gage_design, GageDesign::Nested);
    assert_eq!(restored.alt_life, LifeModel::Lognormal);
    assert_eq!(restored.alt_acceleration, AccelerationModel::InversePower);
    assert!((restored.alt_use_stress - 350.0).abs() < 1e-9);
    assert_eq!(restored.ss_life, LifeModel::Exponential);
    assert_eq!(restored.ss_acceleration, AccelerationModel::Eyring);
    assert!((restored.ss_use_stress - 320.0).abs() < 1e-9);
    assert_eq!(restored.sampling_sample, 80);
    assert_eq!(restored.sampling_accept, 2);
    assert_eq!(restored.sampling_lot, 5000);
    assert!((restored.sampling_aql - 1.5).abs() < 1e-9);
    assert!((restored.sampling_ltpd - 4.0).abs() < 1e-9);
    assert!((restored.sampling_alpha - 5.0).abs() < 1e-9);
    assert!((restored.sampling_beta - 10.0).abs() < 1e-9);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_remaining_confidence_levels_persist() {
    let dir = std::env::temp_dir().join(format!("fp-conf-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.dialog_ci = false;
    app.dialog_ci_level = 90.0;
    app.t_confidence = 98.0;
    app.test2_confidence = 91.0;
    app.ecdf_confidence = 92.0;
    app.interval_confidence = 93.0;
    app.tolerance_confidence = 94.0;
    app.curve_confidence = 96.0;
    app.equivalence_confidence = 97.0;
    app.step_confidence = 90.5;
    app.km_confidence = 95.5;
    app.t2_confidence = 99.0;
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert!(!restored.dialog_ci);
    for (actual, expected) in [
        (restored.dialog_ci_level, 90.0),
        (restored.t_confidence, 98.0),
        (restored.test2_confidence, 91.0),
        (restored.ecdf_confidence, 92.0),
        (restored.interval_confidence, 93.0),
        (restored.tolerance_confidence, 94.0),
        (restored.curve_confidence, 96.0),
        (restored.equivalence_confidence, 97.0),
        (restored.step_confidence, 90.5),
        (restored.km_confidence, 95.5),
        (restored.t2_confidence, 99.0),
    ] {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_control_chart_and_regression_choices_persist() {
    use fp_core::spc::{ControlChartKind, TestSet};

    let dir = std::env::temp_dir().join(format!("fp-chart-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.chart_kind = ControlChartKind::XbarR;
    app.chart_subgroup = 4;
    app.chart_tests = TestSet::only(&[1, 2, 5])
        .expect("tests")
        .with_sensitivity(2, 8)
        .expect("sensitivity");
    app.chart_cusum_k = 0.7;
    app.chart_cusum_h = 5.0;
    app.chart_ewma_lambda = 0.3;
    app.chart_ewma_limit = 2.5;
    app.reg_confidence = 90.0;
    app.anova_confidence = 99.0;
    app.persist_preferences();

    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.chart_kind, ControlChartKind::XbarR);
    assert_eq!(restored.chart_subgroup, 4);
    assert!(restored.chart_tests.is_enabled(1));
    assert!(restored.chart_tests.is_enabled(2));
    assert!(restored.chart_tests.is_enabled(5));
    assert!(!restored.chart_tests.is_enabled(3));
    assert_eq!(restored.chart_tests.sensitivity(2), 8);
    assert!((restored.chart_cusum_k - 0.7).abs() < 1e-9);
    assert!((restored.chart_cusum_h - 5.0).abs() < 1e-9);
    assert!((restored.chart_ewma_lambda - 0.3).abs() < 1e-9);
    assert!((restored.chart_ewma_limit - 2.5).abs() < 1e-9);
    assert!((restored.reg_confidence - 90.0).abs() < 1e-9);
    assert!((restored.anova_confidence - 99.0).abs() < 1e-9);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_batch_folder_can_be_chosen_with_the_native_browser() {
    let mut app = FreePlotsApp::new();
    app.open_batch_dialog();
    app.request_folder_browse("Choose the folder with the CSV files");
    let request = app.take_folder_request().expect("folder request");
    assert_eq!(request.title, "Choose the folder with the CSV files");
    app.set_browsed_folder("/tmp/opencode/csvs".to_owned());
    assert_eq!(app.batch_folder, "/tmp/opencode/csvs");
    assert!(app.take_folder_request().is_none());
}

#[test]
fn the_batch_dialog_requests_a_folder_and_builds_the_summary() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.open_batch_dialog();
    assert!(app.show_batch_dialog);
    app.run_batch_dialog();
    assert!(app.error.is_some(), "the folder is required");
    app.batch_folder = "/tmp/opencode".to_owned();
    app.batch_column = "weight".to_owned();
    app.run_batch_dialog();
    assert!(app.error.is_none(), "{:?}", app.error);
    let request = app.take_batch_request().expect("batch request");
    assert_eq!(request.folder, "/tmp/opencode");
    assert_eq!(request.column, "weight");
    app.apply_batch(
        vec![
            ("a.csv".to_owned(), Ok(frame())),
            ("b.csv".to_owned(), Ok(frame())),
        ],
        "weight",
    );
    let text = app.session_text();
    assert!(text.contains("Batch descriptive statistics"), "{text}");
    assert!(text.contains("a.csv"), "{text}");
    assert_eq!(app.status, "Batch summary: 2 file(s), column weight");
    // An empty folder is refused by the applier, not silently accepted.
    app.apply_batch(Vec::new(), "weight");
    assert!(app.error.is_some());
}

#[test]
fn the_preferences_round_trip_through_the_settings_file() {
    let dir = std::env::temp_dir().join(format!("fp-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");

    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    assert!(!path.exists(), "nothing is written until an option changes");
    app.set_language(fp_core::i18n::Lang::Portuguese);
    app.set_dark_mode(true);
    app.zoom = 1.25;
    app.show_session = false;
    app.show_grid_lines = false;
    app.native_plot_windows = false;
    app.persist_preferences();
    assert!(path.exists(), "the change was not written");
    assert_eq!(app.preferences_error(), None);

    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.language(), fp_core::i18n::Lang::Portuguese);
    assert!(restored.dark_mode);
    assert!((restored.zoom - 1.25).abs() < 1e-9);
    assert!(!restored.show_session);
    assert!(!restored.show_grid_lines);
    assert!(!restored.native_plot_windows);

    // A malformed file keeps the defaults and is replaced on the next save.
    std::fs::write(&path, "{not json").expect("write");
    let mut recovered = FreePlotsApp::new();
    recovered.set_preferences_path(path.clone());
    assert_eq!(recovered.language(), fp_core::i18n::Lang::English);
    assert!(recovered.show_grid_lines);
    recovered.set_dark_mode(true);
    recovered.persist_preferences();
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"dark_mode\": true"), "{text}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn out_of_range_preferences_keep_safe_defaults() {
    use fp_core::attribute::AttributeChartKind;
    use fp_core::hypothesis::Alternative;
    use fp_core::workflow::DistributionKind;

    let dir = std::env::temp_dir().join(format!("fp-invalid-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let invalid = Preferences {
        zoom: 99.0,
        probability_distribution: Some("nonsense".to_owned()),
        capability_confidence: Some(10.0),
        control_chart_subgroup: Some(0),
        control_chart_tests: Some("9".to_owned()),
        doe_runs: Some(999),
        pca_keep: Some(0),
        matrix_bins: Some(1000),
        timeseries_kind: Some("nope".to_owned()),
        sampling_sample: Some(0),
        step_alpha_enter: Some(5.0),
        tolerance_proportion: Some(10.0),
        attribute_kind: Some("nope".to_owned()),
        t_alternative: Some("nope".to_owned()),
        power_groups: Some(1),
        ..Preferences::default()
    };
    let text = serde_json::to_string(&invalid).expect("json");
    // Unknown keys are ignored instead of failing the whole file.
    let text = format!("{}{}", &text[..text.len() - 1], ",\"unknown_field\":42}");
    std::fs::write(&path, text).expect("write");

    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    assert!((app.zoom - 2.5).abs() < 1e-9, "zoom is clamped");
    assert_eq!(app.distribution, DistributionKind::Normal);
    assert!((app.cap_confidence - 95.0).abs() < 1e-9);
    assert_eq!(app.chart_subgroup, 5);
    assert!(app.chart_tests.is_enabled(1));
    assert!(!app.chart_tests.is_enabled(2));
    assert_eq!(app.design_runs, 12);
    assert_eq!(app.pca_keep, 2);
    assert_eq!(app.matrix_bins, 8);
    assert_eq!(app.timeseries_kind, TimeSeriesKind::MovingAverage);
    assert_eq!(app.sampling_sample, 20);
    assert!((app.step_alpha_enter - 0.15).abs() < 1e-9);
    assert!((app.tolerance_proportion - 95.0).abs() < 1e-9);
    assert_eq!(app.attr_kind, AttributeChartKind::P);
    assert_eq!(app.t_alternative, Alternative::TwoSided);
    assert_eq!(app.power_groups, 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn conditional_rules_match_values_and_text() {
    use crate::app::{ConditionalRule, ConditionalStyle, ConditionalTest};

    let rule = |test| ConditionalRule {
        column: "weight".to_owned(),
        test,
        style: ConditionalStyle::RedFill,
    };
    assert!(rule(ConditionalTest::Greater(13.0)).matches(Some(13.1), "13.1"));
    assert!(!rule(ConditionalTest::Greater(13.0)).matches(Some(13.0), "13.0"));
    assert!(!rule(ConditionalTest::Greater(13.0)).matches(None, "*"));
    assert!(rule(ConditionalTest::Less(12.0)).matches(Some(-5.0), "-5"));
    assert!(rule(ConditionalTest::Between(11.0, 13.0)).matches(Some(13.0), "13"));
    assert!(!rule(ConditionalTest::Between(11.0, 13.0)).matches(Some(13.1), "13.1"));
    // Equal to compares numbers when both sides parse, text otherwise.
    assert!(rule(ConditionalTest::EqualTo("13.10".to_owned())).matches(Some(13.1), "13.1"));
    assert!(rule(ConditionalTest::EqualTo("abc".to_owned())).matches(None, "ABC"));
    assert!(rule(ConditionalTest::Contains("crack".to_owned())).matches(None, "hairline crack"));
    assert!(!rule(ConditionalTest::Contains("crack".to_owned())).matches(None, "dent"));
}

#[test]
fn conditional_rules_apply_to_the_worksheet_and_persist() {
    use crate::app::{ConditionalStyle, ConditionalTest};

    let dir = std::env::temp_dir().join(format!("fp-conditional-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);

    app.open_conditional_dialog();
    app.conditional_column = "weight".to_owned();
    app.conditional_kind = ConditionalKind::Greater;
    app.conditional_value = "13".to_owned();
    app.conditional_style = ConditionalStyle::RedFill;
    app.add_conditional_rule();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.conditional_rules.len(), 1);
    let df = app.data().expect("data");
    assert_eq!(
        app.conditional_style(df, 0, 0),
        None,
        "12.5 is not above 13"
    );
    assert_eq!(
        app.conditional_style(df, 0, 1),
        Some(ConditionalStyle::RedFill),
        "13.1 is above 13"
    );

    // An invalid interval is refused and leaves the rules untouched.
    app.conditional_kind = ConditionalKind::Between;
    app.conditional_value = "10".to_owned();
    app.conditional_value2 = "5".to_owned();
    app.add_conditional_rule();
    assert!(app.error.is_some());
    assert_eq!(app.conditional_rules.len(), 1);

    // Text rules work on the label column.
    app.conditional_column = "batch".to_owned();
    app.conditional_kind = ConditionalKind::EqualTo;
    app.conditional_value = "A".to_owned();
    app.conditional_style = ConditionalStyle::Bold;
    app.add_conditional_rule();
    let df = app.data().expect("data");
    assert_eq!(
        app.conditional_style(df, 1, 0),
        Some(ConditionalStyle::Bold)
    );

    // The first matching rule wins.
    app.conditional_column = "weight".to_owned();
    app.conditional_kind = ConditionalKind::Greater;
    app.conditional_value = "11".to_owned();
    app.conditional_style = ConditionalStyle::YellowFill;
    app.add_conditional_rule();
    let df = app.data().expect("data");
    assert_eq!(
        app.conditional_style(df, 0, 1),
        Some(ConditionalStyle::RedFill),
        "the earlier rule must win"
    );

    // Rules are saved with the preferences and survive a restart.
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.conditional_rules.len(), 3);
    let df = frame();
    assert_eq!(
        restored.conditional_style(&df, 0, 1),
        Some(ConditionalStyle::RedFill)
    );

    restored.remove_conditional_rule(0);
    assert_eq!(restored.conditional_rules.len(), 2);
    restored.clear_conditional_rules();
    assert!(restored.conditional_rules.is_empty());
    let _ = ConditionalTest::Greater(1.0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_control_chart_can_use_a_phase_column() {
    let mut app = FreePlotsApp::new();
    let values = vec![10.0, 10.2, 9.8, 10.1, 20.0, 20.2, 19.8, 20.1];
    let phases: Vec<String> = std::iter::repeat_n("before".to_owned(), 4)
        .chain(std::iter::repeat_n("after".to_owned(), 4))
        .collect();
    let df = DataFrame::new(vec![
        Column::new_f64("weight", values),
        Column::new_text("phase", phases),
    ])
    .expect("frame");
    app.set_data(df, "phased.csv".to_owned());
    app.chart_phase_column = "phase".to_owned();
    app.run_control_chart_with_options(
        "weight",
        fp_core::spc::ControlChartKind::IndividualsMovingRange,
        1,
        fp_core::spc::TestSet::default(),
        fp_core::spc::CusumOptions::default(),
        fp_core::spc::EwmaOptions::default(),
    );
    assert!(app.error.is_none(), "{:?}", app.error);
    let text = app.session_text();
    assert!(text.contains("Phases"), "{text}");
    assert!(text.contains("after"), "{text}");
    assert!(app.plot().is_some());
    // The phase column is cleared when a chart family without phases runs.
    app.chart_phase_column.clear();
}

#[test]
fn the_analysis_dialog_choices_persist() {
    use fp_core::workflow::{DistributionKind, PlotDisplay, PlotLayout};

    let dir = std::env::temp_dir().join(format!("fp-analysis-prefs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.distribution = DistributionKind::Weibull;
    app.dialog_distribution = DistributionKind::Weibull;
    app.dialog_display = PlotDisplay::Fit;
    app.dialog_layout = PlotLayout::Overlay;
    app.dialog_same_x = true;
    app.dialog_same_y = true;
    app.cap_distribution = DistributionKind::Gamma;
    app.cap_confidence = 90.0;
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.distribution, DistributionKind::Weibull);
    assert_eq!(restored.dialog_display, PlotDisplay::Fit);
    assert_eq!(restored.dialog_layout, PlotLayout::Overlay);
    assert!(restored.dialog_same_x);
    assert!(restored.dialog_same_y);
    assert_eq!(restored.cap_distribution, DistributionKind::Gamma);
    assert!((restored.cap_confidence - 90.0).abs() < 1e-6);

    // Opening the probability dialog keeps those choices instead of resetting.
    restored.set_data(frame(), "weights.csv".to_owned());
    restored.open_probability_plot_dialog();
    assert_eq!(restored.dialog_layout, PlotLayout::Overlay);
    assert!(restored.dialog_same_x);
    assert_eq!(restored.cap_distribution, DistributionKind::Gamma);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_graph_text_choices_apply_and_persist() {
    let dir = std::env::temp_dir().join(format!("fp-graph-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");

    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");

    app.open_graph_text();
    assert!((app.graph_text_size - 12.0).abs() < 1e-6);
    app.graph_text_family = fp_core::plot::GraphFontFamily::Serif;
    app.graph_text_size = 15.0;
    app.apply_graph_text();
    assert_eq!(
        app.graph_overlay.font_family,
        fp_core::plot::GraphFontFamily::Serif
    );
    assert!((app.graph_overlay.font_scale - 1.25).abs() < 1e-6);
    assert_eq!(app.status, "Graph text updated");
    let svg = fp_core::plot::render_svg_with(app.plot().expect("plot"), app.graph_overlay());
    assert!(svg.contains("DejaVu Serif, serif"), "{svg}");
    assert!(
        svg.contains("font-size=\"20\""),
        "16 pt title becomes 20 pt"
    );

    // The overlay, labels included, is written to the settings file.
    app.graph_overlay.subtitle = Some("Line A".to_owned());
    app.graph_overlay.footer = Some("Night shift".to_owned());
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.graph_overlay.subtitle.as_deref(), Some("Line A"));
    assert_eq!(
        restored.graph_overlay.footer.as_deref(),
        Some("Night shift")
    );
    assert_eq!(
        restored.graph_overlay.font_family,
        fp_core::plot::GraphFontFamily::Serif
    );
    assert!((restored.graph_overlay.font_scale - 1.25).abs() < 1e-6);

    // An out-of-range scale in a hand-edited file is clamped.
    std::fs::write(
        &path,
        r#"{"language":"en","dark_mode":false,"zoom":1.0,"show_session":true,
            "show_grid_lines":true,"native_plot_windows":true,"tour_seen":true,
            "graph_overlay":{"subtitle":null,"footer":null,"reference_lines":[],
            "font_family":"Mono","font_scale":9.0}}"#,
    )
    .expect("write");
    let mut clamped = FreePlotsApp::new();
    clamped.set_preferences_path(path.clone());
    assert!(clamped.graph_overlay.font_scale <= 1.5);
    assert_eq!(
        clamped.graph_overlay.font_family,
        fp_core::plot::GraphFontFamily::Mono
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn marks_are_cleared_by_the_graph_action() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");
    app.brush_marks.lock().expect("marks").push((2, 0));
    assert_eq!(app.brush_marks.lock().expect("marks").len(), 1);
    let ctx = egui::Context::default();
    app.apply_graph_action(&ctx, crate::app::graph_edit::GraphAction::ClearMarks);
    assert!(app.brush_marks.lock().expect("marks").is_empty());
    assert_eq!(app.status, "Marks cleared");
    // A new analysis clears any mark from the previous graph.
    app.brush_marks.lock().expect("marks").push((0, 0));
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");
    assert!(app.brush_marks.lock().expect("marks").is_empty());
}

#[test]
fn notes_can_be_duplicated_rotated_reordered_and_deleted() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");
    let annotation = |text: &str, x: f32, angle: f32| fp_core::plot::Annotation {
        text: text.to_owned(),
        x,
        y: 0.1,
        size: 12.0,
        color: fp_core::plot::AnnotationColor::Black,
        angle,
    };
    app.graph_overlay
        .annotations
        .push(annotation("first", 0.1, 0.0));
    app.graph_overlay
        .annotations
        .push(annotation("second", 0.5, 0.0));

    app.duplicate_note(0);
    assert_eq!(app.graph_overlay.annotations.len(), 3);
    assert_eq!(app.graph_overlay.annotations[2].text, "first");
    assert!(app.graph_overlay.annotations[2].x > 0.1);

    app.rotate_note(0, 15.0);
    app.rotate_note(0, 15.0);
    assert!((app.graph_overlay.annotations[0].angle - 30.0).abs() < 1e-6);
    // The rotation is clamped to -90..90.
    for _ in 0..10 {
        app.rotate_note(0, 15.0);
    }
    assert!((app.graph_overlay.annotations[0].angle - 90.0).abs() < 1e-6);

    app.bring_note_to_front(0);
    assert_eq!(app.graph_overlay.annotations[2].text, "first");
    assert_eq!(app.status, "Note brought to front");

    app.delete_note(0);
    assert_eq!(app.graph_overlay.annotations.len(), 2);
    assert_eq!(app.status, "Note removed");
}

#[test]
fn the_graph_can_be_saved_as_png_and_jpeg() {
    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    app.begin_graph_image(FileKind::Png);
    assert_eq!(app.save_kind, FileKind::Png);
    assert_eq!(app.save_path, "graph.png");
    assert!(app.show_save);
    app.show_save = false;
    app.begin_graph_image(FileKind::Jpeg);
    assert_eq!(app.save_kind, FileKind::Jpeg);
    assert_eq!(app.save_path, "graph.jpg");
    // The raster backends are covered by the fp-app unit tests; here we check
    // that the SVG given to them is a complete document.
    let svg = fp_core::plot::render_svg_with(app.plot().expect("plot"), app.graph_overlay());
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(svg.ends_with("</svg>\n"), "the SVG is not closed");
}

#[test]
fn the_graph_can_be_printed_as_a_pdf_page() {
    let mut app = FreePlotsApp::new();
    app.begin_graph_pdf();
    assert!(app.error.is_some(), "no graph yet");
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");
    app.begin_graph_pdf();
    assert_eq!(app.save_kind, FileKind::GraphPdf);
    assert_eq!(app.save_path, "graph.pdf");
    assert!(app.show_save);
    // The PDF writer accepts an empty session and still draws the graph.
    let bytes = fp_core::pdf::session_pdf("Probability", "", app.plot());
    assert!(bytes.starts_with(b"%PDF-1.4"), "not a PDF");
    assert!(bytes.len() > 1000, "the page is too small: {}", bytes.len());
}

#[test]
fn the_axis_scale_is_validated_and_applied() {
    let spec = fp_core::plot::PlotSpec {
        title: "test".to_owned(),
        x_axis: fp_core::plot::AxisSpec {
            label: "x".to_owned(),
            scale: fp_core::plot::Scale::Linear,
            min: 0.0,
            max: 10.0,
            ticks: Vec::new(),
            minor: Vec::new(),
        },
        y_axis: fp_core::plot::AxisSpec {
            label: "y".to_owned(),
            scale: fp_core::plot::Scale::Linear,
            min: 0.0,
            max: 10.0,
            ticks: Vec::new(),
            minor: Vec::new(),
        },
        size: (600, 400),
        points: vec![fp_core::plot::Point { x: 2.0, y: 3.0 }],
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    };
    let mut app = FreePlotsApp::new();
    app.plot = Some(spec);
    app.open_axis_scale();
    app.axis_x_min = "2".to_owned();
    app.axis_x_max = "8".to_owned();
    app.axis_y_min = "1".to_owned();
    app.axis_y_max = "5".to_owned();
    app.apply_axis_scale(false);
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.graph_overlay.x_range, Some((2.0, 8.0)));
    assert_eq!(app.graph_overlay.y_range, Some((1.0, 5.0)));

    // A reversed interval and a half-filled pair are refused.
    app.open_axis_scale();
    app.axis_x_min = "9".to_owned();
    app.axis_x_max = "2".to_owned();
    app.apply_axis_scale(false);
    assert!(app.error.is_some());
    app.axis_x_min = String::new();
    app.apply_axis_scale(false);
    assert!(app.error.is_some());

    // Automatic clears both ranges.
    app.apply_axis_scale(true);
    assert_eq!(app.graph_overlay.x_range, None);
    assert_eq!(app.graph_overlay.y_range, None);
}

#[test]
fn data_labels_percentiles_layout_and_grid_are_editable() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");

    // Data labels: y value, then removed.
    app.open_data_labels();
    app.data_label_mode = fp_core::plot::DataLabelMode::Index;
    app.apply_data_labels(false);
    assert_eq!(
        app.graph_overlay.data_labels,
        Some(fp_core::plot::DataLabelMode::Index)
    );
    app.open_data_labels();
    app.apply_data_labels(true);
    assert_eq!(app.graph_overlay.data_labels, None);

    // Percentile lines at the 50th percentile are added as reference lines.
    app.open_percentiles();
    app.percentile_axis = fp_core::plot::ReferenceAxis::Horizontal;
    app.percentile_values = "50".to_owned();
    app.add_percentile_lines();
    assert!(app.error.is_none(), "{:?}", app.error);
    let line = app
        .graph_overlay
        .reference_lines
        .last()
        .expect("percentile line");
    assert_eq!(line.label.as_deref(), Some("P50"));
    assert!((line.value - 0.0).abs() < 1e-6, "median z = {}", line.value);
    // An invalid percentile is refused.
    app.open_percentiles();
    app.percentile_values = "101".to_owned();
    app.add_percentile_lines();
    assert!(app.error.is_some());

    // Gridlines toggle.
    let before = app.graph_overlay.grid;
    app.apply_graph_action(
        &egui::Context::default(),
        crate::app::graph_edit::GraphAction::GridToggle,
    );
    assert_ne!(app.graph_overlay.grid, before);

    // Panel layout only works on a panel graph.
    app.open_panel_layout();
    assert!(app.error.is_some(), "a single plot has no panels");
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::control_chart(
            "weight",
            fp_core::spc::ControlChartKind::IndividualsMovingRange,
            1,
        ),
        "I-MR".to_owned(),
    )
    .expect("chart");
    app.open_panel_layout();
    app.panel_columns_draft = 2;
    app.apply_panel_layout();
    assert_eq!(app.plot().map(|plot| plot.panel_columns), Some(2));
    assert_eq!(app.status, "Panel layout: 2 column(s)");
}

#[test]
fn graph_annotations_are_placed_and_kept() {
    let dir = std::env::temp_dir().join(format!("fp-annotation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");

    app.open_graph_annotation();
    app.add_graph_annotation();
    assert!(app.error.is_some(), "empty text is refused");
    assert!(app.show_graph_annotation, "the dialog stays open");

    app.annotation_text = "Night shift".to_owned();
    app.annotation_x = 50.0;
    app.annotation_y = 25.0;
    app.annotation_size = 16.0;
    app.annotation_color = fp_core::plot::AnnotationColor::Red;
    app.annotation_angle = 90.0;
    app.add_graph_annotation();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_eq!(app.graph_overlay.annotations.len(), 1);
    let annotation = &app.graph_overlay.annotations[0];
    assert_eq!(annotation.text, "Night shift");
    assert!((annotation.x - 0.5).abs() < 1e-6);
    assert!((annotation.y - 0.25).abs() < 1e-6);
    assert!((annotation.size - 16.0).abs() < 1e-6);
    assert_eq!(annotation.color, fp_core::plot::AnnotationColor::Red);
    assert!((annotation.angle - 90.0).abs() < 1e-6);
    assert_eq!(app.status, "Note added to the graph");

    // The placement reaches the SVG and survives a restart.
    let svg = fp_core::plot::render_svg_with(app.plot().expect("plot"), app.graph_overlay());
    assert!(svg.contains("Night shift"), "{svg}");
    app.persist_preferences();
    let mut restored = FreePlotsApp::new();
    restored.set_preferences_path(path.clone());
    assert_eq!(restored.graph_overlay.annotations.len(), 1);
    assert_eq!(restored.graph_overlay.annotations[0].text, "Night shift");

    // Out-of-range positions are clamped instead of dropping the text.
    restored.annotation_text = "edge".to_owned();
    restored.annotation_x = 250.0;
    restored.annotation_y = -40.0;
    restored.add_graph_annotation();
    let edge = restored.graph_overlay.annotations.last().expect("edge");
    assert!((edge.x - 1.0).abs() < 1e-6);
    assert!((edge.y - 0.0).abs() < 1e-6);

    // Reset clears every annotation.
    restored.reset_graph_edits();
    assert!(restored.graph_overlay.annotations.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resetting_the_graph_edits_also_resets_the_text_style() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::probability_plot("weight"),
        "Probability".to_owned(),
    )
    .expect("plot");
    app.graph_overlay.subtitle = Some("Line A".to_owned());
    app.graph_overlay.font_family = fp_core::plot::GraphFontFamily::Mono;
    app.graph_overlay.font_scale = 1.5;
    app.reset_graph_edits();
    assert_eq!(
        app.graph_overlay.font_family,
        fp_core::plot::GraphFontFamily::Sans
    );
    assert!((app.graph_overlay.font_scale - 1.0).abs() < 1e-6);
    assert_eq!(app.graph_overlay.subtitle, None);
}

#[test]
fn the_help_menu_opens_the_settings_and_agents_dialogs() {
    use egui_kittest::{Harness, kittest::Queryable};

    let dir = std::env::temp_dir().join(format!("fp-help-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let mut app = FreePlotsApp::new();
    app.set_preferences_path(dir.join("settings.json"));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    harness.get_by_label("Help").click();
    harness.run();
    harness
        .get_by_label("Where the settings are stored…")
        .click();
    harness.run();
    harness.get_by_label("Where the settings are stored");
    harness.get_by_label_contains("settings.json");
    harness.get_by_label("Copy path").click();
    harness.step();
    let copied = harness.output().platform_output.commands.iter().any(|command| {
        matches!(command, egui::OutputCommand::CopyText(text) if text.ends_with("settings.json"))
    });
    assert!(copied, "the path did not reach the clipboard");
    harness.state_mut().show_settings_info = false;
    harness.run();

    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Assistant for agents…").click();
    harness.run();
    harness.get_by_label("Assistant for agents");
    harness.get_by_label("FreePlots --mcp");
    harness.get_by_label("FreePlots --mcp-http 127.0.0.1:8767");
    harness.get_by_label("Copy configuration").click();
    harness.step();
    let config = harness.output().platform_output.commands.iter().any(|command| {
        matches!(command, egui::OutputCommand::CopyText(text) if text.contains("mcpServers"))
    });
    assert!(
        config,
        "the client configuration did not reach the clipboard"
    );
    harness.state_mut().show_agents_dialog = false;
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_tour_walks_through_the_window_and_closes() {
    use egui_kittest::{Harness, kittest::Queryable};

    let app = FreePlotsApp::new();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Show the tour").click();
    harness.run();
    assert!(harness.state().show_tour);
    assert_eq!(harness.state().tour_step, 0);
    harness.get_by_label("The worksheet");
    harness.get_by_label("Next").click();
    harness.run();
    assert_eq!(harness.state().tour_step, 1);
    harness.get_by_label("The toolbar");
    harness.get_by_label("Back").click();
    harness.run();
    assert_eq!(harness.state().tour_step, 0);
    // Walk to the end and finish.
    for _ in 0..5 {
        harness.get_by_label("Next").click();
        harness.run();
    }
    harness.get_by_label("Finish").click();
    harness.run();
    assert!(!harness.state().show_tour);
}

#[test]
fn every_graph_context_menu_action_is_audited() {
    use graph_edit::GraphAction;

    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    let ctx = egui::Context::default();

    // Each editor opens its own dialog.
    app.apply_graph_action(&ctx, GraphAction::Labels);
    assert!(app.show_graph_labels);
    app.show_graph_labels = false;
    app.apply_graph_action(&ctx, GraphAction::Text);
    assert!(app.show_graph_text);
    app.show_graph_text = false;
    let text_size = app.graph_overlay.text_size(12.0);
    app.apply_graph_action(&ctx, GraphAction::LargerText);
    assert!(app.graph_overlay.text_size(12.0) > text_size);
    app.apply_graph_action(&ctx, GraphAction::SmallerText);
    assert!((app.graph_overlay.text_size(12.0) - text_size).abs() < 1e-6);
    app.apply_graph_action(&ctx, GraphAction::DataDisplay);
    assert!(app.show_data_display);
    app.show_data_display = false;
    app.apply_graph_action(&ctx, GraphAction::Annotation);
    assert!(app.show_graph_annotation);
    app.show_graph_annotation = false;
    app.apply_graph_action(&ctx, GraphAction::Notes);
    assert!(app.show_notes);
    app.show_notes = false;
    app.load_sample("Scatter with fit");
    app.apply_graph_action(&ctx, GraphAction::DataLabels);
    assert!(app.show_data_labels);
    app.show_data_labels = false;
    app.apply_graph_action(&ctx, GraphAction::Reference);
    assert!(app.show_reference_line);
    app.show_reference_line = false;
    app.apply_graph_action(&ctx, GraphAction::Percentiles);
    assert!(app.show_percentiles);
    app.show_percentiles = false;
    app.apply_graph_action(&ctx, GraphAction::AxisScale);
    assert!(app.show_axis_scale);
    app.show_axis_scale = false;

    // Panel layout needs panels; on a single plot it explains the refusal.
    app.apply_graph_action(&ctx, GraphAction::PanelLayout);
    assert!(app.error.is_some(), "a single plot has no panel layout");
    app.load_sample("Filling line capability");
    app.run_spec_blocking(
        fp_core::workflow::AnalysisSpec::control_chart(
            "diameter",
            fp_core::spc::ControlChartKind::IndividualsMovingRange,
            1,
        ),
        "I-MR".to_owned(),
    )
    .expect("chart");
    app.apply_graph_action(&ctx, GraphAction::PanelLayout);
    assert!(app.show_panel_layout);
    app.show_panel_layout = false;

    // Gridlines toggle both ways and leave the default on.
    let report = app.graph_overlay.show_report;
    app.apply_graph_action(&ctx, GraphAction::ReportToggle);
    assert_ne!(app.graph_overlay.show_report, report);
    app.apply_graph_action(&ctx, GraphAction::ReportToggle);
    assert_eq!(app.graph_overlay.show_report, report);

    let before = app.graph_overlay.grid;
    app.apply_graph_action(&ctx, GraphAction::GridToggle);
    assert_ne!(app.graph_overlay.grid, before);
    app.apply_graph_action(&ctx, GraphAction::GridToggle);
    assert_eq!(app.graph_overlay.grid, before);

    // Save and print open the save dialog with the right format.
    app.apply_graph_action(&ctx, GraphAction::Save);
    assert_eq!(app.save_kind, FileKind::Svg);
    assert!(app.show_save);
    app.show_save = false;
    app.apply_graph_action(&ctx, GraphAction::Print);
    assert_eq!(app.save_kind, FileKind::GraphPdf);
    assert_eq!(app.save_path, "graph.pdf");
    assert!(app.show_save);
    app.show_save = false;
    app.apply_graph_action(&ctx, GraphAction::SavePng);
    assert_eq!(app.save_kind, FileKind::Png);
    assert_eq!(app.save_path, "graph.png");
    app.show_save = false;
    app.apply_graph_action(&ctx, GraphAction::SaveJpeg);
    assert_eq!(app.save_kind, FileKind::Jpeg);
    assert_eq!(app.save_path, "graph.jpg");
    app.show_save = false;

    // Copy puts the SVG on the clipboard (the status is what the user sees;
    // the platform command is covered by the copy UI test).
    app.apply_graph_action(&ctx, GraphAction::Copy);
    assert_eq!(app.status, "Graph copied as SVG");

    // Marks and the whole overlay are cleared.
    app.brush_marks.lock().expect("marks").push((0, 0));
    app.apply_graph_action(&ctx, GraphAction::ClearMarks);
    assert!(app.brush_marks.lock().expect("marks").is_empty());
    assert_eq!(app.status, "Marks cleared");

    app.graph_overlay.subtitle = Some("Line A".to_owned());
    app.graph_overlay
        .annotations
        .push(fp_core::plot::Annotation {
            text: "note".to_owned(),
            ..fp_core::plot::Annotation::default()
        });
    app.graph_overlay.x_range = Some((1.0, 2.0));
    app.apply_graph_action(&ctx, GraphAction::Reset);
    assert_eq!(app.graph_overlay, fp_core::plot::GraphOverlay::default());
    assert_eq!(app.status, "Graph edits reset");
}

#[test]
fn the_graph_context_menu_changes_are_visible() {
    use fp_core::plot::{
        Annotation, DataLabelMode, GraphFontFamily, ReferenceAxis, ReferenceLine, render_svg_with,
    };
    use graph_edit::GraphAction;

    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    let ctx = egui::Context::default();
    let svg = |app: &FreePlotsApp| render_svg_with(app.plot().expect("plot"), app.graph_overlay());
    let cpk_baseline = svg(&app);

    // Gridlines and the report block are drawn by the overlay, so each
    // toggle must change the SVG and restore it exactly.
    app.apply_graph_action(&ctx, GraphAction::GridToggle);
    assert_ne!(svg(&app), cpk_baseline, "gridlines did not change the SVG");
    app.apply_graph_action(&ctx, GraphAction::GridToggle);
    assert_eq!(svg(&app), cpk_baseline);

    app.apply_graph_action(&ctx, GraphAction::ReportToggle);
    assert_ne!(
        svg(&app),
        cpk_baseline,
        "report data did not change the SVG"
    );
    app.apply_graph_action(&ctx, GraphAction::ReportToggle);
    assert_eq!(svg(&app), cpk_baseline);

    // The label editor changes the title, subtitle and axis labels.
    app.apply_graph_action(&ctx, GraphAction::Labels);
    app.graph_label_subtitle = "audit".to_owned();
    app.apply_graph_labels();
    assert_ne!(
        svg(&app),
        cpk_baseline,
        "graph labels did not change the SVG"
    );
    app.apply_graph_action(&ctx, GraphAction::Reset);
    assert_eq!(svg(&app), cpk_baseline);

    // The graph-text editor changes the typeface and the size.
    app.apply_graph_action(&ctx, GraphAction::Text);
    app.graph_text_family = GraphFontFamily::Mono;
    app.graph_text_size = 16.0;
    app.apply_graph_text();
    assert_ne!(svg(&app), cpk_baseline, "graph text did not change the SVG");
    app.apply_graph_action(&ctx, GraphAction::Reset);
    assert_eq!(svg(&app), cpk_baseline);

    // The context menu can also step the font size up and down directly.
    app.apply_graph_action(&ctx, GraphAction::LargerText);
    assert_ne!(
        svg(&app),
        cpk_baseline,
        "larger text did not change the SVG"
    );
    app.apply_graph_action(&ctx, GraphAction::SmallerText);
    assert_eq!(svg(&app), cpk_baseline);

    // The capability histogram has bars, not points: labels and percentiles
    // are not applicable and the menu hides them, while a scatter has points.
    assert!(!app.plot().expect("plot").has_points());
    app.apply_graph_action(&ctx, GraphAction::DataLabels);
    assert!(app.error.is_some());
    app.error = None;
    app.apply_graph_action(&ctx, GraphAction::Percentiles);
    assert!(app.error.is_some());
    app.error = None;

    app.load_sample("Scatter with fit");
    assert!(app.plot().expect("plot").has_points());
    let scatter_baseline = svg(&app);

    // Data labels, annotations, reference lines, percentiles and a manual
    // axis range each add visible ink on a plot with points.
    app.graph_overlay.data_labels = Some(DataLabelMode::YValue);
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "data labels did not change the SVG"
    );
    app.graph_overlay.data_labels = None;

    app.graph_overlay.annotations.push(Annotation {
        text: "audit".to_owned(),
        ..Annotation::default()
    });
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "annotation did not change the SVG"
    );
    app.graph_overlay.annotations.clear();

    let plot = app.plot().expect("plot");
    let (x_min, x_max) = (plot.x_axis.min, plot.x_axis.max);
    let (y_min, y_max) = (plot.y_axis.min, plot.y_axis.max);
    app.graph_overlay.reference_lines.push(ReferenceLine {
        axis: ReferenceAxis::Vertical,
        value: (x_min + x_max) / 2.0,
        label: Some("ref".to_owned()),
    });
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "reference line did not change the SVG"
    );
    app.graph_overlay.reference_lines.clear();

    app.open_percentiles();
    app.percentile_values = "50".to_owned();
    app.add_percentile_lines();
    assert!(app.error.is_none(), "{:?}", app.error);
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "percentile line did not change the SVG"
    );
    app.graph_overlay.reference_lines.clear();

    app.apply_graph_action(&ctx, GraphAction::AxisScale);
    app.axis_x_min = format!("{}", x_min + (x_max - x_min) * 0.25);
    app.axis_x_max = format!("{}", x_min + (x_max - x_min) * 0.75);
    app.axis_y_min = format!("{}", y_min + (y_max - y_min) * 0.25);
    app.axis_y_max = format!("{}", y_min + (y_max - y_min) * 0.75);
    app.apply_axis_scale(false);
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "axis range did not change the SVG"
    );
    app.apply_graph_action(&ctx, GraphAction::Reset);
    assert_eq!(svg(&app), scatter_baseline);

    // Hiding symbols/lines/bands removes ink from the drawing.
    app.graph_overlay.show_symbols = false;
    assert_ne!(
        svg(&app),
        scatter_baseline,
        "symbols did not change the SVG"
    );
    app.graph_overlay.show_symbols = true;
    app.graph_overlay.show_lines = false;
    assert_ne!(svg(&app), scatter_baseline, "lines did not change the SVG");
    app.graph_overlay.show_lines = true;
    assert_eq!(svg(&app), scatter_baseline);

    // The panel editor changes the column count of a multi-panel plot.
    app.load_sample("Three probability plots");
    let panel_baseline = svg(&app);
    let columns = app.plot().expect("plot").panel_columns;
    app.apply_graph_action(&ctx, GraphAction::PanelLayout);
    assert!(app.show_panel_layout);
    app.panel_columns_draft = if columns > 1 { 1 } else { 3 };
    app.apply_panel_layout();
    assert_ne!(
        svg(&app),
        panel_baseline,
        "panel layout did not change the SVG"
    );
    app.plot.as_mut().expect("plot").panel_columns = columns;
    app.show_panel_layout = false;

    // A probability plot carries a confidence band; hiding it changes the SVG.
    app.load_sample("Two probability plots");
    let band_baseline = svg(&app);
    app.graph_overlay.show_bands = false;
    assert_ne!(svg(&app), band_baseline, "bands did not change the SVG");
    app.graph_overlay.show_bands = true;
    assert_eq!(svg(&app), band_baseline);
}

#[test]
fn graph_actions_reject_a_missing_graph_and_bad_values() {
    use graph_edit::GraphAction;

    let mut app = FreePlotsApp::new();
    let ctx = egui::Context::default();
    for action in [
        GraphAction::Labels,
        GraphAction::Text,
        GraphAction::DataDisplay,
        GraphAction::Annotation,
        GraphAction::Notes,
        GraphAction::DataLabels,
        GraphAction::Reference,
        GraphAction::Percentiles,
        GraphAction::AxisScale,
        GraphAction::PanelLayout,
        GraphAction::Save,
        GraphAction::SavePng,
        GraphAction::SaveJpeg,
        GraphAction::Print,
    ] {
        app.apply_graph_action(&ctx, action);
        if !matches!(action, GraphAction::Notes | GraphAction::DataDisplay) {
            assert!(app.error.is_some(), "{action:?} accepted an empty app");
        }
        app.error = None;
    }

    // Reference line: a non-numeric value is refused and adds nothing.
    app.load_sample("Filling line capability");
    app.open_reference_line();
    app.reference_value = "abc".to_owned();
    app.add_reference_line();
    assert!(app.error.is_some());
    assert!(app.graph_overlay.reference_lines.is_empty());

    // Percentiles out of range are refused.
    app.open_percentiles();
    app.percentile_values = "0, 100".to_owned();
    app.add_percentile_lines();
    assert!(app.error.is_some());
    assert!(app.graph_overlay.reference_lines.is_empty());

    // Note management on an empty list is harmless.
    app.duplicate_note(3);
    app.delete_note(3);
    app.rotate_note(3, 15.0);
    app.bring_note_to_front(3);
    assert!(app.graph_overlay.annotations.is_empty());
}

#[test]
fn the_graph_actions_work_from_the_window_menu() {
    use graph_edit::GraphAction;

    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    let ctx = egui::Context::default();

    app.apply_graph_action(&ctx, GraphAction::Labels);
    assert!(app.show_graph_labels);
    app.show_graph_labels = false;

    app.apply_graph_action(&ctx, GraphAction::Reference);
    assert!(app.show_reference_line);
    app.show_reference_line = false;

    app.reference_value = "25.1".to_owned();
    app.add_reference_line();
    assert_eq!(app.graph_overlay().reference_lines.len(), 1);

    app.apply_graph_action(&ctx, GraphAction::Copy);
    assert_eq!(app.status, "Graph copied as SVG");

    app.apply_graph_action(&ctx, GraphAction::Save);
    assert_eq!(app.save_kind, FileKind::Svg);

    app.apply_graph_action(&ctx, GraphAction::Reset);
    assert!(app.graph_overlay().reference_lines.is_empty());
}

#[test]
fn saving_the_graph_asks_for_an_svg_file() {
    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    app.begin_plot_save();
    assert!(app.show_save);
    assert_eq!(app.save_kind, FileKind::Svg);
    assert_eq!(app.save_path, "graph.svg");

    // The confirmation names the graph, not the session, and leaves the
    // worksheet source alone.
    let source = app.source().to_owned();
    app.mark_graph_saved("graph.svg");
    assert_eq!(app.status(), "Graph saved to graph.svg");
    assert_eq!(app.source(), source);
}

#[test]
fn the_graph_labels_and_reference_lines_reach_the_renderer() {
    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    let plot = app.plot.clone().expect("capability plot");

    app.open_graph_labels();
    assert_eq!(app.graph_label_title, plot.title);
    app.graph_label_title = "My capability".to_owned();
    app.graph_label_subtitle = "Line 3 - night shift".to_owned();
    app.graph_label_footer = "Operator: A. Silva".to_owned();
    app.graph_label_x = "Diameter (mm)".to_owned();
    app.graph_label_y = "Frequency".to_owned();
    app.apply_graph_labels();
    let edited = app.plot.clone().expect("plot");
    assert_eq!(edited.title, "My capability");
    assert_eq!(edited.x_axis.label, "Diameter (mm)");
    assert_eq!(edited.y_axis.label, "Frequency");
    assert_eq!(
        app.graph_overlay.subtitle.as_deref(),
        Some("Line 3 - night shift")
    );
    assert_eq!(
        app.graph_overlay.footer.as_deref(),
        Some("Operator: A. Silva")
    );

    app.open_reference_line();
    app.reference_value = "25.0".to_owned();
    app.reference_label = "target".to_owned();
    app.add_reference_line();
    assert_eq!(app.graph_overlay.reference_lines.len(), 1);
    let svg = fp_core::plot::render_svg_with(&edited, &app.graph_overlay);
    assert!(svg.contains("My capability"));
    assert!(svg.contains("Line 3 - night shift"));
    assert!(svg.contains("Operator: A. Silva"));
    assert!(svg.contains(">target<"));
    assert!(svg.contains("stroke-dasharray"));

    // Reset restores the automatic labels and clears the edits.
    app.reset_graph_edits();
    let back = app.plot.clone().expect("plot");
    assert_eq!(back.title, plot.title);
    assert_eq!(back.x_axis.label, plot.x_axis.label);
    assert!(app.graph_overlay.reference_lines.is_empty());
    assert_eq!(app.graph_overlay.subtitle, None);
    assert_eq!(app.graph_overlay.footer, None);
}

#[test]
fn the_tour_runs_once_and_the_flag_persists() {
    let dir = std::env::temp_dir().join(format!("fp-tour-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("settings.json");

    let mut app = FreePlotsApp::new();
    app.set_preferences_path(path.clone());
    app.maybe_show_tour();
    assert!(app.show_tour, "the first run opens the tour");
    assert!(app.tour_seen);
    // Closing it keeps the flag, and the next start stays quiet.
    app.show_tour = false;
    app.persist_preferences();
    let mut next = FreePlotsApp::new();
    next.set_preferences_path(path.clone());
    next.maybe_show_tour();
    assert!(!next.show_tour, "the tour must not open on later runs");
    assert!(next.tour_seen);

    // Help can still open it explicitly.
    next.begin_tour();
    assert!(next.show_tour);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_sample_gallery_loads_a_project() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.open_samples_dialog();
    assert!(app.show_samples_dialog);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    let button = harness
        .get_all_by_label("Weibull capability (PPM)")
        .next()
        .expect("sample button");
    button.click();
    harness.run();
    harness.run();
    assert!(!harness.state().show_samples_dialog);
    assert!(harness.state().session_text().contains("Capability"));
    assert!(harness.state().data().unwrap().column("life").is_ok());
}

#[test]
fn every_sample_project_loads_and_runs() {
    for (label, _, _) in FreePlotsApp::sample_projects() {
        let mut app = FreePlotsApp::new();
        app.load_sample(label);
        assert!(app.error.is_none(), "{label}: {:?}", app.error);
        assert!(
            app.data().is_some_and(|df| df.n_rows() > 0),
            "{label}: no worksheet data"
        );
        assert!(!app.session_text().is_empty(), "{label}: no Session report");
    }
}

#[test]
fn the_sample_projects_load_and_run_an_analysis() {
    let mut app = FreePlotsApp::new();
    app.load_sample("Filling line capability");
    assert_eq!(app.data().map(DataFrame::n_rows), Some(120));
    assert!(
        app.session_text().contains("Capability"),
        "{}",
        app.session_text()
    );
    assert!(app.data().unwrap().column("diameter").is_ok());

    app.load_sample("Gage R&R (3 operators)");
    assert_eq!(app.data().map(DataFrame::n_rows), Some(90));
    assert!(
        app.session_text().contains("Gage R&R"),
        "{}",
        app.session_text()
    );

    app.load_sample("Screening experiment (2^2)");
    assert_eq!(app.data().map(DataFrame::n_rows), Some(12));
    assert!(app.session_text().contains("A*B"), "{}", app.session_text());
}

#[test]
fn the_third_party_notices_are_in_the_binary_and_in_help() {
    use egui_kittest::{Harness, kittest::Queryable};

    // The embedded text is what ships with every copy.
    assert!(super::licences::THIRD_PARTY_NOTICES.contains("MIT License"));
    assert!(super::licences::THIRD_PARTY_NOTICES.contains("Bitstream Vera"));
    assert!(
        super::licences::THIRD_PARTY_NOTICES
            .contains("endorsed by any commercial statistical software vendor")
    );

    let app = FreePlotsApp::new();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.get_by_label("Help").click();
    harness.run();
    harness.get_by_label("Third-party licences…").click();
    harness.run();
    assert!(harness.state().show_licences);
    // The dialog renders the notice text; the first line is its heading.
    harness.get_by_label_contains("FreePlots - third-party notices");
}

#[test]
fn the_about_and_help_follow_the_language() {
    use egui_kittest::{Harness, kittest::Queryable};

    let app = FreePlotsApp::new();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // English by default, including the description and the licence.
    harness.state_mut().show_about = true;
    harness.run();
    harness.get_by_label(
        "Statistics and analysis software written in Rust by AI, accessible to everyone.",
    );
    harness.get_by_label("MIT license");
    harness.get_by_label("Visit the public FreePlots repository");
    assert_eq!(
        harness
            .query_all_by_label(
                "Software de análise e estatística escrito em Rust por IA, acessível a todos."
            )
            .count(),
        0
    );

    // The same dialog in Portuguese, with no English left in it.
    harness
        .state_mut()
        .set_language(fp_core::i18n::Lang::Portuguese);
    harness.run();
    harness.get_by_label(
        "Software de análise e estatística escrito em Rust por IA, acessível a todos.",
    );
    harness.get_by_label("Licença MIT");
    harness.get_by_label("Visite o repositório público do FreePlots");
    harness.get_by_label("Motor FreePlots 0.1.0");
    assert_eq!(
        harness.query_all_by_label("FreePlots Engine 0.1.0").count(),
        0
    );
    assert_eq!(harness.query_all_by_label("MIT license").count(), 0);
    assert_eq!(
        harness
            .query_all_by_label(
                "Statistics and analysis software written in Rust by AI, accessible to everyone."
            )
            .count(),
        0
    );

    // The help window follows the same catalogue.
    harness.state_mut().show_about = false;
    harness.state_mut().show_shortcuts = true;
    harness.run();
    harness.get_by_label("Anular e repetir");
    harness
        .state_mut()
        .set_language(fp_core::i18n::Lang::English);
    harness.run();
    harness.get_by_label("Undo and redo");
    assert_eq!(harness.query_all_by_label("Anular e repetir").count(), 0);
    harness.get_by_label(
        "Ctrl and Shift combinations use the keyboard layout in use; the grid never records keys while a text field has focus.",
    );
    harness
        .state_mut()
        .set_language(fp_core::i18n::Lang::Portuguese);
    harness.run();
    harness.get_by_label(
        "As combinações com Ctrl e Shift seguem o layout do teclado em uso; a grelha nunca capta teclas enquanto um campo de texto tem o foco.",
    );

    // The assistant goals are translated too, while the action is still
    // chosen by index.
    harness.state_mut().show_shortcuts = false;
    harness.state_mut().show_assistant_dialog = true;
    harness.run();
    harness.get_by_label("Comparar dois grupos");
    harness.get_by_label("Planear uma experiência");
}

#[test]
fn the_cells_expose_their_painted_text_to_accessibility() {
    use egui_kittest::{Harness, kittest::Queryable};

    let df = DataFrame::new(vec![Column::new_f64("weight", vec![1234.5678, 2.0])]).expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // Unformatted: the address and the value as the grid paints it.
    harness.get_by_label("C1 R1: 1234.5678");
    harness.get_by_label("C1 R2: 2");
    // Empty cells still have an address, so a screen reader can navigate.
    harness.get_by_label("C2 R1");

    // The accessibility label follows the column format, exactly like the
    // painted text: this is the live check the plan asked for.
    harness.state_mut().formats.insert(
        "weight".to_owned(),
        ColumnFormat {
            decimals: 1,
            thousands: true,
            bold: false,
            align: CellAlign::Right,
        },
    );
    harness.run();
    harness.get_by_label("C1 R1: 1 234.6");
    assert_eq!(harness.query_all_by_label("C1 R1: 1234.5678").count(), 0);
}

#[test]
fn the_formula_bar_has_an_accessible_name() {
    use egui_kittest::{Harness, kittest::Queryable};

    let df = DataFrame::new(vec![Column::new_f64("weight", vec![1.0, 2.0])]).expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // The "fx" label names the formula bar, so a screen reader announces the
    // text input instead of an unnamed edit.
    harness.get_by_role_and_label(accesskit::Role::TextInput, "Formula bar");
    // Start from an empty field: the bar opens with the selected cell's value.
    harness.state_mut().edit_buffer.clear();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Formula bar")
        .click();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Formula bar")
        .type_text("=1+1");
    harness.run();
    assert_eq!(harness.state().edit_buffer, "=1+1");
}

#[test]
fn saving_over_an_existing_file_asks_first() {
    use egui_kittest::{Harness, kittest::Queryable};

    let dir = std::env::temp_dir().join(format!("fp-overwrite-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let existing = dir.join("report.csv");
    std::fs::write(&existing, "old").expect("fixture");
    let fresh = dir.join("fresh.csv");

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    // A new path goes straight through.
    harness.state_mut().begin_save(FileKind::Csv);
    harness.state_mut().save_path = fresh.display().to_string();
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .take_save_request()
            .map(|request| request.path),
        Some(fresh.display().to_string())
    );

    // An existing path waits for confirmation; Cancel writes nothing.
    harness.state_mut().begin_save(FileKind::Csv);
    harness.state_mut().save_path = existing.display().to_string();
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    assert!(harness.state().show_overwrite);
    assert!(harness.state().save_request.is_none());
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(!harness.state().show_overwrite);
    assert!(harness.state().save_request.is_none());

    // Replace emits the request; the window is gone.
    harness.state_mut().begin_save(FileKind::Csv);
    harness.state_mut().save_path = existing.display().to_string();
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    harness.get_by_label("Replace").click();
    harness.run();
    assert!(!harness.state().show_overwrite);
    let request = harness.state_mut().take_save_request().expect("request");
    assert_eq!(request.path, existing.display().to_string());
    assert_eq!(request.kind, FileKind::Csv);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_assistant_opens_the_right_dialog_for_each_goal() {
    use egui_kittest::{Harness, kittest::Queryable};

    // Two numeric columns: the DOE and the two-group paths need them.
    let df = DataFrame::new(vec![
        Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
        Column::new_f64("length", vec![20.1, 20.4, 19.8, 20.2, 20.0]),
        Column::new_text("batch", vec!["A".to_string(); 5]),
    ])
    .expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();

    let goals = [
        ("Describe my data", "session"),
        ("Compare two groups", "t"),
        ("Compare three or more groups", "anova"),
        ("Test a proportion", "test2"),
        ("Check a process (control chart)", "chart"),
        ("Measure process capability", "cap"),
        ("Fit a regression", "reg"),
        ("Design an experiment", "doe"),
        ("Test normality", "probability"),
    ];
    for (goal, expected) in goals {
        harness.state_mut().show_assistant_dialog = true;
        harness.run();
        harness.get_by_label(goal).click();
        harness.run();
        let state = harness.state();
        assert!(
            !state.show_assistant_dialog,
            "{goal} kept the assistant open"
        );
        let opened = match expected {
            "session" => !state.session.is_empty(),
            "t" => state.show_t_dialog,
            "anova" => state.show_anova_dialog,
            "test2" => state.show_test2_dialog,
            "chart" => state.show_chart_dialog,
            "cap" => state.show_cap_dialog,
            "reg" => state.show_reg_dialog,
            "doe" => state.show_doe_dialog,
            "probability" => state.show_probability_plot_dialog,
            other => panic!("unknown expectation {other}"),
        };
        assert!(opened, "{goal} did not open the {expected} path");
        // Close whatever opened so the next goal starts clean.
        let state = harness.state_mut();
        state.show_t_dialog = false;
        state.show_anova_dialog = false;
        state.show_test2_dialog = false;
        state.show_chart_dialog = false;
        state.show_cap_dialog = false;
        state.show_reg_dialog = false;
        state.show_doe_dialog = false;
        state.show_probability_plot_dialog = false;
        harness.run();
    }
}

#[test]
fn find_through_the_dialog_selects_the_cell() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
        .type_text("13.1");
    harness.run();
    assert_eq!(harness.state().find_text, "13.1");
    harness.get_by_label("Find Next").click();
    harness.run();
    assert_eq!(harness.state().selected_cell(), Some((0, 1)));
}

#[test]
fn the_grid_renders_a_large_frame_with_twenty_six_columns() {
    use egui_kittest::{Harness, kittest::Queryable};

    let values: Vec<f64> = (0..200_000).map(|index| index as f64 * 0.001).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "big.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 200_000);
    // The sheet offers 26 columns even though the data has one. Only the
    // columns inside the viewport are laid out: the twentieth is on
    // screen, the twenty-sixth is beyond it and waits for a scroll, which
    // is what lets the grid keep a thousand columns cheap.
    assert_eq!(harness.get_all_by_label("C1").count(), 1);
    assert_eq!(harness.get_all_by_label("C15").count(), 1);
    let laid_out = (1..=26)
        .filter(|index| harness.query_all_by_label(&format!("C{index}")).count() > 0)
        .count();
    assert!(
        (10..26).contains(&laid_out),
        "{laid_out} headers laid out of 26"
    );
    assert_eq!(harness.query_all_by_label("C26").count(), 0);
    harness.hover_at(egui::pos2(600.0, 300.0));
    harness.run();
    assert_eq!(harness.state().data().expect("frame").n_rows(), 200_000);
}

#[test]
fn dragging_a_divider_widens_and_keeps_the_column() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    let before = harness
        .state()
        .column_widths
        .first()
        .copied()
        .unwrap_or(72.0);
    // The divider between the row header and C1 sits at x = 127.
    harness.hover_at(egui::pos2(127.0, 140.0));
    harness.run();
    harness.drag_at(egui::pos2(127.0, 140.0));
    harness.event(egui::Event::PointerMoved(egui::pos2(165.0, 140.0)));
    harness.drop_at(egui::pos2(165.0, 140.0));
    harness.run();
    let after = harness
        .state()
        .column_widths
        .first()
        .copied()
        .unwrap_or(0.0);
    assert!(after > before, "before {before}, after {after}");
}

#[test]
fn adding_and_deleting_columns_is_undoable() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_column();
    assert_eq!(app.data().expect("frame").n_columns(), 3);
    app.undo();
    assert_eq!(app.data().expect("frame").n_columns(), 2);

    app.add_text_column();
    assert_eq!(app.data().expect("frame").n_columns(), 3);
    app.undo();
    assert_eq!(app.data().expect("frame").n_columns(), 2);

    app.select_cell(1, 0);
    app.delete_column();
    assert_eq!(app.data().expect("frame").n_columns(), 1);
    app.undo();
    let data = app.data().expect("frame");
    assert_eq!(data.n_columns(), 2);
    assert_eq!(data.columns()[1].name(), "batch");
}

#[test]
fn sorting_reorders_the_worksheet() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.open_sort_dialog();
    app.sort_keys = "weight:desc".to_owned();
    app.run_sort_dialog();
    let data = app.data().expect("frame");
    assert_eq!(data.cell_text(0, 0).as_deref(), Some("13.1"));
    assert_eq!(data.cell_text(0, 1).as_deref(), Some("12.8"));
}

#[test]
fn a_big_paste_is_one_undo_group() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut block = String::new();
    for row in 0..200 {
        for column in 0..10 {
            if column > 0 {
                block.push('\t');
            }
            block.push_str(&format!("{}", row * 10 + column));
        }
        block.push('\n');
    }
    app.paste_tsv(&block);
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 200);
    assert_eq!(data.n_columns(), 10);
    assert_eq!(data.cell_text(9, 199).as_deref(), Some("1999"));
    app.undo();
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 5);
    assert_eq!(data.n_columns(), 2);
}

#[test]
fn the_sheets_keep_their_own_data_and_undo() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.set_edit_buffer("99");
    assert!(app.commit_cell());
    let first = app.data().expect("frame").cell_text(0, 0);
    assert_eq!(first.as_deref(), Some("99"));

    app.add_sheet();
    assert_eq!(app.sheet_names().len(), 2);
    assert_eq!(app.data().expect("new sheet").n_rows(), 0);

    app.switch_sheet(0);
    assert_eq!(app.data().expect("first").cell_text(0, 0), first);
    app.undo();
    assert_eq!(
        app.data().expect("first").cell_text(0, 0).as_deref(),
        Some("12.5")
    );
    app.switch_sheet(1);
    assert!(app.data().expect("second").n_rows() == 0);
    assert!(app.close_sheet(1));
    assert_eq!(app.sheet_names().len(), 1);
}

#[test]
fn the_context_menu_copies_and_cuts() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.state_mut().select_cell(1, 0);
    harness.state_mut().extend_selection(1, 1);
    harness.hover_at(egui::pos2(150.0, 170.0));
    harness.run();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: egui::pos2(150.0, 170.0),
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    harness.get_by_label("Copy").click();
    harness.step();
    assert_eq!(harness.state().clipboard, "A\nA");
    // The same text goes to the system clipboard.
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .any(|command| {
            matches!(
                command,
                egui::OutputCommand::CopyText(text) if text == "A\nA"
            )
        });
    assert!(copied, "no CopyText command reached the platform");
    harness.run();

    harness.state_mut().select_cell(1, 0);
    harness.state_mut().extend_selection(1, 1);
    harness.get_by_label("Edit").click();
    harness.run();
    harness.get_by_label("Cut").click();
    harness.run();
    let data = harness.state().data().expect("frame");
    assert!(data.cell_is_missing(1, 0));
    assert!(data.cell_is_missing(1, 1));
}

#[test]
fn a_formula_in_the_cell_is_evaluated_on_commit() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.set_edit_buffer("=SUM(C1R1:C1R5)");
    assert!(app.commit_cell(), "{:?}", app.error);
    assert_eq!(
        app.data().expect("frame").cell_text(0, 0).as_deref(),
        Some("62.5")
    );

    app.set_edit_buffer("=ROUND(2.345,2)");
    assert!(app.commit_cell());
    assert_eq!(
        app.data().expect("frame").cell_text(0, 0).as_deref(),
        Some("2.35")
    );

    // A broken formula is refused and the cell keeps its value.
    app.set_edit_buffer("=SUM(");
    assert!(!app.commit_cell());
    assert!(app.error.is_some());
    assert_eq!(
        app.data().expect("frame").cell_text(0, 0).as_deref(),
        Some("2.35")
    );
}

#[test]
fn a_background_analysis_finishes_and_reports() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.set_background_analyses(true);
    app.run_spec(
        AnalysisSpec::descriptive_statistics("weight"),
        "Describe".to_owned(),
    )
    .expect("started");
    let ctx = egui::Context::default();
    let started = std::time::Instant::now();
    while app.analysis_running() && started.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        app.poll_analysis(&ctx);
    }
    assert!(!app.analysis_running(), "the worker did not finish");
    assert!(
        app.session_text().contains("Descriptive"),
        "{}",
        app.session_text()
    );
    assert_eq!(app.status(), "Describe");
    assert!(app.error.is_none());
}

#[test]
fn a_second_analysis_is_refused_while_one_runs() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.set_background_analyses(true);
    // White-box: pretend a worker is running.
    let (sender, receiver) = std::sync::mpsc::channel();
    app.pending_analysis = Some(receiver);
    let outcome = app.run_spec(
        AnalysisSpec::descriptive_statistics("weight"),
        "Describe".to_owned(),
    );
    assert!(outcome.is_err());
    assert!(
        app.error
            .as_deref()
            .unwrap_or("")
            .contains("already running")
    );
    drop(sender);
}

#[test]
fn a_project_carries_every_worksheet() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.add_sheet();
    app.set_data(frame(), "batch.csv".to_owned());
    let sheets = app.project_sheets();
    assert_eq!(sheets.len(), 2, "{sheets:?}");
    assert_eq!(sheets[0].0, "Worksheet 1");
    assert_eq!(sheets[1].0, "Worksheet 2");

    let json = fp_core::io::to_project_json_sheets(&sheets, "job.fpp").expect("write");
    let back = fp_core::io::from_project_json_sheets(&json).expect("read");
    let mut restored = FreePlotsApp::new();
    restored.load_project_sheets(back, "job.fpp".to_owned());
    assert_eq!(restored.sheets.len(), 2);
    assert_eq!(restored.sheets[0].name, "Worksheet 1");
    assert_eq!(restored.sheets[1].name, "Worksheet 2");
    assert_eq!(
        restored.sheets[1].data.as_ref().map(DataFrame::n_rows),
        Some(5)
    );
    assert_eq!(restored.active_sheet, 0);
    // The second tab is reachable and keeps its data.
    restored.switch_sheet(1);
    assert_eq!(restored.data().map(DataFrame::n_rows), Some(5));
    assert_eq!(restored.sheets[1].name, "Worksheet 2");
}

#[test]
fn a_refused_paste_changes_nothing() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.paste_tsv("abc");
    assert!(app.error.as_deref().is_some_and(|error| !error.is_empty()));
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 5);
    assert_eq!(data.n_columns(), 2);
    assert_eq!(data.cell_text(0, 0).as_deref(), Some("12.5"));
    assert!(!app.can_undo(), "a refused paste must not leave history");

    // A block that fails on its second row is rolled back as a whole.
    app.select_cell(0, 0);
    app.paste_tsv("1\t2\nabc\t4");
    assert!(app.error.is_some());
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 5);
    assert_eq!(data.cell_text(0, 0).as_deref(), Some("12.5"));
    assert_eq!(data.cell_text(1, 0).as_deref(), Some("A"));
    assert!(!app.can_undo());

    // Columns added for the paste are dropped again when it fails.
    app.select_cell(1, 0);
    app.paste_tsv("a\tb\tc\t\td\te\tabc");
    assert!(app.error.is_some());
    assert_eq!(app.data().expect("frame").n_columns(), 2);
}

#[test]
fn a_windows_clipboard_block_pastes_without_carriage_returns() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.select_cell(0, 0);
    app.paste_tsv("1\t2\r\n3\t4\r\n");
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 5);
    assert_eq!(data.cell_text(0, 0).as_deref(), Some("1"));
    assert_eq!(data.cell_text(1, 0).as_deref(), Some("2"));
    assert_eq!(data.cell_text(0, 1).as_deref(), Some("3"));
    assert_eq!(data.cell_text(1, 1).as_deref(), Some("4"));
}

#[test]
fn a_paste_of_ten_thousand_cells_takes_under_two_seconds() {
    use std::time::Instant;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut block = String::new();
    for row in 0..1_000 {
        for column in 0..10 {
            if column > 0 {
                block.push('\t');
            }
            block.push_str(&(row * 10 + column).to_string());
        }
        block.push('\n');
    }
    let started = Instant::now();
    app.paste_tsv(&block);
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "10 000 cells took {elapsed:?}"
    );
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 1_000);
    assert_eq!(data.n_columns(), 10);
    assert_eq!(data.cell_text(9, 999).as_deref(), Some("9999"));
    // One undo returns the whole block to the frame that was there.
    app.undo();
    let data = app.data().expect("frame");
    assert_eq!(data.n_rows(), 5);
    assert_eq!(data.n_columns(), 2);
}

#[test]
fn the_session_trims_itself_at_the_memory_limit() {
    let mut app = FreePlotsApp::new();
    // Each "report" is about a mebibyte; twenty of them exceed the cap.
    let block = "x".repeat(1 << 20);
    for index in 0..20 {
        app.append_session(&format!("REPORT {index}\n{block}"));
    }
    assert!(
        app.session.len() <= 16 << 20,
        "session grew to {} bytes",
        app.session.len()
    );
    assert!(
        app.session.contains("REPORT 19"),
        "the newest report was trimmed away"
    );
    assert!(
        !app.session.contains("REPORT 0"),
        "the oldest report should be gone"
    );
}

#[test]
fn a_thousand_columns_render_without_stalling() {
    use egui_kittest::{Harness, kittest::Queryable};

    let rows = 100;
    let columns = 1_000;
    let data: Vec<Column> = (0..columns)
        .map(|column| {
            Column::new_f64(
                format!("C{}", column + 1),
                (0..rows)
                    .map(|row| (row * columns + column) as f64)
                    .collect(),
            )
        })
        .collect();
    let df = DataFrame::new(data).expect("frame");
    let mut app = FreePlotsApp::new();
    app.set_data(df, "wide.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    let started = std::time::Instant::now();
    harness.run();
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the first frame took {elapsed:?}"
    );
    assert_eq!(
        harness.state().data().map(DataFrame::n_columns),
        Some(1_000)
    );
    // Only a screenful of headers is laid out: the far columns are built
    // when the grid scrolls to them, which is what keeps the frame cheap.
    let laid_out = (1..=columns)
        .filter(|index| harness.query_all_by_label(&format!("C{index}")).count() > 0)
        .count();
    assert!(
        laid_out < 40,
        "{laid_out} of {columns} column headers were laid out"
    );
    assert_eq!(harness.query_all_by_label("C1").count(), 1);
    assert_eq!(harness.query_all_by_label("C1000").count(), 0);
}

#[test]
fn replace_all_through_the_dialog_and_undo() {
    use egui_kittest::{Harness, kittest::Queryable};

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
        .type_text("A");
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Replace with:")
        .click();
    harness.run();
    harness
        .get_by_role_and_label(accesskit::Role::TextInput, "Replace with:")
        .type_text("B");
    harness.run();
    harness.get_by_label("Replace All").click();
    harness.run();
    assert_eq!(harness.state().find_message, "5 replaced");
    for row in 0..5 {
        assert_eq!(
            harness
                .state()
                .data()
                .expect("frame")
                .cell_text(1, row)
                .as_deref(),
            Some("B")
        );
    }
    harness.state_mut().undo();
    for row in 0..5 {
        assert_eq!(
            harness
                .state()
                .data()
                .expect("frame")
                .cell_text(1, row)
                .as_deref(),
            Some("A")
        );
    }
}

#[test]
fn the_column_widths_belong_to_each_worksheet() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    app.column_widths = vec![100.0; 26];
    app.add_sheet();
    assert!(
        app.column_widths.is_empty(),
        "new sheet starts with defaults"
    );
    app.column_widths = vec![140.0; 26];
    app.switch_sheet(0);
    assert_eq!(app.column_widths[0], 100.0, "first sheet keeps its widths");
    app.switch_sheet(1);
    assert_eq!(app.column_widths[0], 140.0, "second sheet keeps its widths");
}

#[test]
fn a_hundred_edits_undo_and_redo_in_order() {
    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let rows = app.data().expect("frame").n_rows();
    for step in 0..100 {
        app.select_cell(0, step);
        app.set_edit_buffer(format!("{}", 500 + step));
        assert!(app.commit_cell(), "edit {step} was refused");
    }
    let last = app
        .data()
        .expect("frame")
        .cell_text(0, 99)
        .unwrap_or_default();
    assert_eq!(last, "599");
    assert_eq!(app.data().map(DataFrame::n_rows), Some(100));
    for _ in 0..100 {
        app.undo();
    }
    assert!(!app.can_undo());
    assert_eq!(app.data().map(DataFrame::n_rows), Some(rows));
    assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    for _ in 0..100 {
        app.redo();
    }
    assert!(!app.can_redo());
    assert_eq!(app.data().unwrap().cell_text(0, 99).as_deref(), Some("599"));
}

#[test]
fn the_grid_remembers_its_column_widths() {
    use egui_kittest::Harness;

    let mut app = FreePlotsApp::new();
    app.set_data(frame(), "weights.csv".to_owned());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, app| app.show(ui), app);
    harness.run();
    let widths = &harness.state().column_widths;
    assert_eq!(widths.len(), 26);
    assert!(widths.iter().all(|width| *width > 0.0));
}
