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

    pub(super) fn load_sample(&mut self, label: &str) {
        let (df, source) = match label {
            "Filling line capability" => (sample_capability(), "filling-line.csv"),
            "Gage R&R (3 operators)" => (sample_gage(), "gage-rr.csv"),
            "Screening experiment (2^2)" => (sample_doe(), "screening.csv"),
            _ => return,
        };
        self.set_data(df, source.to_owned());
        let spec = match label {
            "Filling line capability" => {
                AnalysisSpec::capability("diameter", Some(24.7), Some(25.3))
            }
            "Gage R&R (3 operators)" => AnalysisSpec::gage_rr("measure", "part", "operator"),
            _ => AnalysisSpec::factorial("y", vec!["A".to_owned(), "B".to_owned()]),
        };
        let _ = self.run_spec_blocking(spec, label.to_owned());
        self.status = format!("Sample project: {label}");
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
