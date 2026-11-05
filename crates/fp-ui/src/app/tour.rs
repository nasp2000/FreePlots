//! First-run tour and the built-in sample projects.

use super::*;

impl FreePlotsApp {
    /// Opens the tour at its first step.
    pub fn begin_tour(&mut self) {
        self.show_tour = true;
        self.tour_step = 0;
        self.tour_seen = true;
    }

    /// Shows the tour automatically on the first run only.
    ///
    /// The flag is part of the preferences, so every later start stays quiet;
    /// Help → Show the tour opens it again at any time.
    pub fn maybe_show_tour(&mut self) {
        if !self.tour_seen {
            self.begin_tour();
        }
    }

    pub(super) fn tour_window(&mut self, ctx: &egui::Context) {
        if !self.show_tour {
            return;
        }
        let steps: [(&str, &str); 6] = [
            (
                "The worksheet",
                "Click any cell and type; Enter commits the value or a formula such as =SUM(C1R1:C1R10). Drag the column dividers to widen a column, and use the arrows at the bottom to switch between worksheets.",
            ),
            (
                "The toolbar",
                "Open a CSV or Excel file, save the worksheet or the project, compute a column, undo and redo. The formula bar above the grid edits the selected cell.",
            ),
            (
                "The menus",
                "Stat holds every analysis, Graph the plots and Assistant suggests one from a plain question. Hover over any menu item to read what it does.",
            ),
            (
                "The Session",
                "Every report appears here, in the same order as the analyses. Clear Session empties it; the logs travel with the exported PDF or HTML.",
            ),
            (
                "The plots",
                "Plots open in their own window: right-click for the labels, reference lines and saving, and close it to keep working.",
            ),
            (
                "Help",
                "This tour lives under Help, together with the keyboard shortcuts, the settings path, the MCP guide and the third-party notices.",
            ),
        ];
        let mut open = self.show_tour;
        let mut finished = false;
        let total = steps.len();
        let step = self.tour_step.min(total - 1);
        let (title, text) = steps[step];
        dialog_window(self.catalog().label("Tour"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.strong(RichText::new(self.catalog().label(title)).size(15.0));
                ui.add_space(4.0);
                ui.label(self.catalog().label(text));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if step > 0 && ui.help_button(self.catalog().label("Back")).clicked() {
                        self.tour_step -= 1;
                    }
                    if step + 1 < total {
                        if ui.help_button(self.catalog().label("Next")).clicked() {
                            self.tour_step += 1;
                        }
                    } else if ui.help_button(self.catalog().label("Finish")).clicked() {
                        finished = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("{} / {}", step + 1, total));
                    });
                });
            });
        self.show_tour = open && !finished;
    }

    /// Built-in sample projects shown in Help > Sample projects.
    pub fn sample_projects() -> &'static [(&'static str, &'static str, &'static str)] {
        SAMPLE_PROJECTS
    }

    /// Loads one sample project and runs its analysis.
    pub(super) fn load_sample(&mut self, label: &str) {
        let Some((source, spec)) = sample_spec(label) else {
            return;
        };
        self.set_data(sample_data(label), source.to_owned());
        let _ = self.run_spec_blocking(spec, label.to_owned());
        self.status = format!("Sample project: {label}");
    }

    /// Opens the sample gallery.
    pub fn open_samples_dialog(&mut self) {
        self.show_samples_dialog = true;
    }

    pub(super) fn samples_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_samples_dialog {
            return;
        }
        let mut open = self.show_samples_dialog;
        let mut chosen: Option<&'static str> = None;
        let catalog = self.catalog();
        dialog_window(catalog.label("Sample projects"))
            .without_clear()
            .open(&mut open)
            .resizable(false)
            .default_width(760.0)
            .show(ctx, |ui| {
                ui.small(catalog.label(
                    "Each sample opens a worksheet and runs the analysis, ready to explore.",
                ));
                let mut category = "";
                for (label, group, description) in SAMPLE_PROJECTS {
                    if *group != category {
                        ui.add_space(8.0);
                        ui.strong(catalog.label(group));
                        category = group;
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .help_button(catalog.label(label))
                            .on_hover_text(catalog.label(description))
                            .clicked()
                        {
                            chosen = Some(label);
                        }
                    });
                }
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Close")).clicked() {
                        self.show_samples_dialog = false;
                    }
                });
            });
        self.show_samples_dialog = open && self.show_samples_dialog;
        if let Some(label) = chosen {
            self.show_samples_dialog = false;
            self.load_sample(label);
        }
    }
}

/// The sample projects: label, category and one-line description.
const SAMPLE_PROJECTS: &[(&str, &str, &str)] = &[
    (
        "Filling line capability",
        "Capability",
        "Cpk/Ppk of 120 diameters, LSL 24.7 and USL 25.3.",
    ),
    (
        "Weibull capability (PPM)",
        "Capability",
        "Positive lifetimes with LSL 4 and USL 12, fitted by Weibull.",
    ),
    (
        "Gage R&R (3 operators)",
        "Measurement systems",
        "Crossed study: 10 parts, 3 operators, 3 trials.",
    ),
    (
        "Screening experiment (2^2)",
        "Design of experiments",
        "Two-factor screening design with a real interaction.",
    ),
    (
        "Response surface (CCD)",
        "Design of experiments",
        "Central composite design with a quadratic response.",
    ),
    (
        "I-MR control chart",
        "Control charts",
        "40 measurements with a small process shift.",
    ),
    (
        "X-bar - R chart (subgroups of 5)",
        "Control charts",
        "30 subgroups of five measurements.",
    ),
    (
        "Attribute P chart",
        "Control charts",
        "Defect counts from 25 samples of 100 units.",
    ),
    (
        "Two probability plots",
        "Graphs",
        "Two columns on separate probability panels.",
    ),
    (
        "Three probability plots",
        "Graphs",
        "Three columns on separate probability panels.",
    ),
    (
        "Matrix plot (3 variables)",
        "Graphs",
        "Pairwise scatterplots with histograms on the diagonal.",
    ),
    (
        "Scatter with fit",
        "Graphs",
        "Correlation with a least-squares line.",
    ),
    (
        "Histogram of diameters",
        "Graphs",
        "Frequency histogram with a normal overlay.",
    ),
    (
        "Pareto of defects",
        "Quality tools",
        "Defect categories sorted by frequency.",
    ),
    (
        "Regression / calibration",
        "Regression",
        "Calibration line with measurement noise.",
    ),
    (
        "Kaplan-Meier reliability",
        "Reliability",
        "Right-censored survival times.",
    ),
    (
        "Time series decomposition",
        "Time series",
        "36 months with trend and seasonality.",
    ),
];

/// Worksheets and analyses behind the sample labels.
fn sample_spec(label: &str) -> Option<(&'static str, AnalysisSpec)> {
    Some(match label {
        "Filling line capability" => (
            "filling-line.csv",
            AnalysisSpec::capability("diameter", Some(24.7), Some(25.3)),
        ),
        "Weibull capability (PPM)" => (
            "bearing-life.csv",
            AnalysisSpec::capability("life", Some(4.0), Some(12.0))
                .with_capability_distribution(DistributionKind::Weibull),
        ),
        "Gage R&R (3 operators)" => (
            "gage-rr.csv",
            AnalysisSpec::gage_rr("measure", "part", "operator"),
        ),
        "Screening experiment (2^2)" => (
            "screening.csv",
            AnalysisSpec::factorial("y", vec!["A".to_owned(), "B".to_owned()]),
        ),
        "Response surface (CCD)" => (
            "response-surface.csv",
            AnalysisSpec::response_surface("y", vec!["A".to_owned(), "B".to_owned()]),
        ),
        "I-MR control chart" => (
            "line-imr.csv",
            AnalysisSpec::control_chart("measurement", ControlChartKind::IndividualsMovingRange, 1),
        ),
        "X-bar - R chart (subgroups of 5)" => (
            "line-xbar.csv",
            AnalysisSpec::control_chart("measurement", ControlChartKind::XbarR, 5),
        ),
        "Attribute P chart" => (
            "defects.csv",
            AnalysisSpec::attribute_chart(AttributeChartKind::P, "defects")
                .with_attribute_size(None, Some(100)),
        ),
        "Two probability plots" => (
            "two-lines.csv",
            AnalysisSpec::probability_plot_multi(vec!["line_a".to_owned(), "line_b".to_owned()]),
        ),
        "Three probability plots" => (
            "three-lines.csv",
            AnalysisSpec::probability_plot_multi(vec![
                "line_a".to_owned(),
                "line_b".to_owned(),
                "line_c".to_owned(),
            ]),
        ),
        "Matrix plot (3 variables)" => (
            "three-variables.csv",
            AnalysisSpec::matrix_plot(vec!["x".to_owned(), "y".to_owned(), "z".to_owned()]),
        ),
        "Scatter with fit" => ("scatter.csv", AnalysisSpec::scatterplot("x", "y")),
        "Histogram of diameters" => ("diameters.csv", AnalysisSpec::histogram("diameter")),
        "Pareto of defects" => ("defect-log.csv", AnalysisSpec::pareto("defect")),
        "Regression / calibration" => (
            "calibration.csv",
            AnalysisSpec::regression("y", vec!["x".to_owned()]),
        ),
        "Kaplan-Meier reliability" => (
            "survival.csv",
            AnalysisSpec::kaplan_meier("time").with_censoring(Some("event".to_owned()), 0.0),
        ),
        "Time series decomposition" => (
            "monthly.csv",
            AnalysisSpec::decomposition(
                "value",
                12,
                fp_core::timeseries::DecompositionModel::Additive,
            ),
        ),
        _ => return None,
    })
}

fn sample_data(label: &str) -> DataFrame {
    match label {
        "Filling line capability" | "Histogram of diameters" => sample_capability(),
        "Weibull capability (PPM)" => sample_weibull_life(),
        "Gage R&R (3 operators)" => sample_gage(),
        "Screening experiment (2^2)" => sample_doe(),
        "Response surface (CCD)" => sample_rsm(),
        "I-MR control chart" => sample_imr(),
        "X-bar - R chart (subgroups of 5)" => sample_xbar_r(),
        "Attribute P chart" => sample_attribute(),
        "Two probability plots" => sample_probability(2),
        "Three probability plots" => sample_probability(3),
        "Matrix plot (3 variables)" => sample_matrix(),
        "Scatter with fit" => sample_scatter(),
        "Pareto of defects" => sample_pareto(),
        "Regression / calibration" => sample_regression(),
        "Kaplan-Meier reliability" => sample_survival(),
        "Time series decomposition" => sample_timeseries(),
        _ => DataFrame::blank(1, 0).expect("blank sample"),
    }
}

/// 120 diameters around 25.00 mm with a slow drift.
fn sample_capability() -> DataFrame {
    let values = (0..120)
        .map(|index| {
            let drift = 0.006 * (index as f64 / 119.0);
            25.0 + drift + 0.075 * ((index as f64 * 12.9898).sin())
        })
        .collect();
    DataFrame::new(vec![Column::new_f64("diameter", values)]).expect("sample")
}

/// Positive, right-skewed lifetimes for a Weibull capability study.
fn sample_weibull_life() -> DataFrame {
    let values = (1..=80)
        .map(|index| {
            let u = f64::from(index) / 81.0;
            let shape = 1.8;
            4.0 + 3.0 * (-(1.0 - u).ln()).powf(1.0 / shape)
        })
        .collect();
    DataFrame::new(vec![Column::new_f64("life", values)]).expect("sample")
}

/// Ten parts, three operators, three replicates: a crossed Gage R&R.
fn sample_gage() -> DataFrame {
    let mut measure = Vec::new();
    let mut part = Vec::new();
    let mut operator = Vec::new();
    for piece in 1..=10 {
        let reference = 10.0 + 0.4 * f64::from(piece);
        for (who, shift) in [("A", 0.0), ("B", 0.03), ("C", -0.02)] {
            for replicate in 0..3 {
                let noise = 0.02 * ((f64::from(piece) * 3.7 + f64::from(replicate)) * 1.7).sin();
                measure.push(reference + shift + noise);
                part.push(format!("P{piece}"));
                operator.push(who.to_owned());
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("measure", measure),
        Column::new_text("part", part),
        Column::new_text("operator", operator),
    ])
    .expect("sample")
}

/// A 2² screening design with three replicates and a real interaction.
fn sample_doe() -> DataFrame {
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for level_a in [-1.0, 1.0] {
        for level_b in [-1.0, 1.0] {
            for replicate in 0..3 {
                a.push(level_a);
                b.push(level_b);
                y.push(
                    50.0 + 4.0 * level_a
                        + 3.0 * level_b
                        + 2.0 * level_a * level_b
                        + 0.05 * f64::from(replicate),
                );
            }
        }
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
    ])
    .expect("sample")
}

/// Central composite design with a curved response and a small interaction.
fn sample_rsm() -> DataFrame {
    let design = [
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
        (1.414, 0.0),
        (-1.414, 0.0),
        (0.0, 1.414),
        (0.0, -1.414),
        (0.0, 0.0),
        (0.0, 0.0),
        (0.0, 0.0),
        (0.0, 0.0),
    ];
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut y = Vec::new();
    for (index, (level_a, level_b)) in design.iter().enumerate() {
        a.push(*level_a);
        b.push(*level_b);
        let noise = 0.12 * ((index as f64 * 1.7).sin());
        y.push(
            50.0 + 3.0 * level_a + 2.0 * level_b
                - 2.0 * level_a * level_a
                - 1.5 * level_b * level_b
                + 1.2 * level_a * level_b
                + noise,
        );
    }
    DataFrame::new(vec![
        Column::new_f64("A", a),
        Column::new_f64("B", b),
        Column::new_f64("y", y),
    ])
    .expect("sample")
}

/// 40 measurements with a step in the mean at observation 25.
fn sample_imr() -> DataFrame {
    let values = (0..40)
        .map(|index| {
            let shift = if index >= 25 { 1.2 } else { 0.0 };
            10.0 + shift + 0.35 * ((index as f64) * 0.9).sin() + 0.15 * ((index as f64) * 2.3).cos()
        })
        .collect();
    DataFrame::new(vec![Column::new_f64("measurement", values)]).expect("sample")
}

/// 30 subgroups of five measurements, with a late shift.
fn sample_xbar_r() -> DataFrame {
    let mut measurement = Vec::new();
    for subgroup in 0..30 {
        let center = 10.0 + 0.02 * f64::from(subgroup) + if subgroup >= 20 { 0.8 } else { 0.0 };
        for repeat in 0..5 {
            let index = f64::from(subgroup * 5 + repeat);
            measurement.push(center + 0.25 * (index * 1.3).sin());
        }
    }
    DataFrame::new(vec![Column::new_f64("measurement", measurement)]).expect("sample")
}

/// 25 samples of 100 units with a few defect counts.
fn sample_attribute() -> DataFrame {
    let defects = (0..25).map(|index| f64::from((index * 7) % 6)).collect();
    DataFrame::new(vec![Column::new_f64("defects", defects)]).expect("sample")
}

/// Two or three columns for separate probability panels.
fn sample_probability(count: usize) -> DataFrame {
    let mut columns = Vec::new();
    for line in 0..count {
        let offset = 20.0 + 0.8 * line as f64;
        let values = (0..40)
            .map(|index| {
                offset
                    + 0.8 * ((index as f64) * 1.1).sin()
                    + 0.3 * ((index as f64) * 0.37 + line as f64).cos()
            })
            .collect();
        columns.push(Column::new_f64(
            format!("line_{}", (b'a' + line as u8) as char),
            values,
        ));
    }
    DataFrame::new(columns).expect("sample")
}

/// Three related variables for a matrix plot.
fn sample_matrix() -> DataFrame {
    let mut x = Vec::new();
    let mut y = Vec::new();
    let mut z = Vec::new();
    for index in 0..40 {
        let value = f64::from(index);
        x.push(value);
        y.push(2.0 * value + 3.0 * (value * 0.7).sin());
        z.push(-value + 2.0 * (value * 0.4).cos());
    }
    DataFrame::new(vec![
        Column::new_f64("x", x),
        Column::new_f64("y", y),
        Column::new_f64("z", z),
    ])
    .expect("sample")
}

/// Two variables with a clear linear relationship.
fn sample_scatter() -> DataFrame {
    let mut x = Vec::new();
    let mut y = Vec::new();
    for index in 0..30 {
        let value = f64::from(index) + 1.0;
        x.push(value);
        y.push(4.0 + 1.8 * value + 2.0 * (value * 1.3).sin());
    }
    DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("sample")
}

/// Defect categories with the usual long tail.
fn sample_pareto() -> DataFrame {
    let mut defect = Vec::new();
    for (name, count) in [
        ("Scratch", 22),
        ("Dent", 14),
        ("Crack", 9),
        ("Stain", 5),
        ("Other", 3),
    ] {
        for _ in 0..count {
            defect.push(name.to_owned());
        }
    }
    DataFrame::new(vec![Column::new_text("defect", defect)]).expect("sample")
}

/// A calibration line with measurement noise.
fn sample_regression() -> DataFrame {
    let mut x = Vec::new();
    let mut y = Vec::new();
    for index in 1..=25 {
        let value = f64::from(index);
        x.push(value);
        y.push(5.0 + 2.0 * value + 0.6 * (value * 1.9).sin());
    }
    DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("sample")
}

/// Right-censored survival times (0 marks a censored unit).
fn sample_survival() -> DataFrame {
    let mut time = Vec::new();
    let mut event = Vec::new();
    for index in 0..30 {
        let value = f64::from(index);
        time.push(5.0 + 1.1 * value + 0.4 * (value * 0.9).sin());
        event.push(f64::from(u8::from(index % 4 != 0)));
    }
    DataFrame::new(vec![
        Column::new_f64("time", time),
        Column::new_f64("event", event),
    ])
    .expect("sample")
}

/// 36 monthly values with a linear trend and a seasonal pattern.
fn sample_timeseries() -> DataFrame {
    let values = (0..36)
        .map(|index| {
            let value = f64::from(index);
            20.0 + 0.3 * value
                + 2.0 * (std::f64::consts::TAU * value / 12.0).sin()
                + 0.4 * (value * 1.7).cos()
        })
        .collect();
    DataFrame::new(vec![Column::new_f64("value", values)]).expect("sample")
}
