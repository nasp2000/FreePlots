//! Design of experiments: factorial, Box-Behnken, Plackett-Burman, Taguchi,
//! response surface, desirability and step-stress.

use super::*;

impl FreePlotsApp {
    pub fn open_doe_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let numeric: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.len() < 2 {
            self.set_error("Factorial analysis needs a response and at least one factor");
            return;
        }
        self.doe_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.doe_factors = numeric
            .into_iter()
            .filter(|name| *name != self.doe_response)
            .collect();
        self.show_doe_dialog = true;
    }
    pub fn run_doe_dialog(&mut self) {
        if self.doe_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .doe_factors
            .iter()
            .filter(|name| **name != self.doe_response)
            .cloned()
            .collect();
        if factors.is_empty() {
            self.set_error("Select at least one factor");
            return;
        }
        let mut spec = AnalysisSpec::factorial(self.doe_response.clone(), factors)
            .with_main_effects(self.doe_main_effects);
        if !self.doe_block.is_empty() {
            spec = spec.with_block(self.doe_block.clone());
        }
        self.execute_spec(spec, format!("Factorial: {}", self.doe_response));
        self.show_doe_dialog = false;
    }
    pub(super) fn doe_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_doe_dialog {
            return;
        }
        let (numeric, all_columns): (Vec<String>, Vec<String>) = self
            .data
            .as_ref()
            .map(|df| {
                let numeric = df
                    .columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect();
                let all = df
                    .columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect();
                (numeric, all)
            })
            .unwrap_or_default();
        let mut open = self.show_doe_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("2-Level Factorial"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("doe_response")
                        .selected_text(if self.doe_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.doe_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.doe_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Factors (coded −1/+1):");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.doe_response {
                                continue;
                            }
                            let mut selected = self.doe_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.doe_factors.push(name.clone());
                                } else {
                                    self.doe_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Block:");
                    egui::ComboBox::from_id_salt("doe_block")
                        .selected_text(if self.doe_block.is_empty() {
                            "None".to_owned()
                        } else {
                            self.doe_block.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.doe_block, String::new(), "None");
                            for name in &all_columns {
                                ui.selectable_value(
                                    &mut self.doe_block,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("All main effects and interactions are fitted simultaneously.");
                ui.small(
                    "At least two residual degrees of freedom are required (replicate the design).",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_doe_dialog = open && !run;
        if run {
            self.run_doe_dialog();
        }
    }
    pub fn open_design_dialog(&mut self) {
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_design_dialog = true;
    }
    pub fn run_design_dialog(&mut self) {
        let factors: Vec<String> = self
            .design_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.is_empty() {
            self.set_error("Enter at least one factor name");
            return;
        }
        let block_size = (self.design_block_size > 0).then_some(self.design_block_size);
        let df = match fp_core::doe::blocked_factorial_design(
            &factors,
            self.design_replicates,
            self.design_centers,
            block_size,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "DOE design".to_owned());
        self.append_session(&format!(
            "DOE design generated: {rows} runs, factors {}.\n",
            factors.join(", ")
        ));
        self.show_design_dialog = false;
    }
    pub(super) fn design_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_design_dialog {
            return;
        }
        let mut open = self.show_design_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Create 2-Level Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.design_factors)
                            .desired_width(220.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Comma-separated factor names; values are generated as −1/+1.");
                ui.horizontal(|ui| {
                    let label = ui.help_label("Replicates:");
                    ui.add(egui::DragValue::new(&mut self.design_replicates).range(1..=100))
                        .labelled_by(label.id).hover_from(&label);
                    let label = ui.help_label("Center points:");
                    ui.add(egui::DragValue::new(&mut self.design_centers).range(0..=100))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Block size:");
                    ui.add(egui::DragValue::new(&mut self.design_block_size).range(0..=100))
                        .labelled_by(label.id).hover_from(&label);
                    ui.small("0 = no blocks");
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.small("This replaces the current worksheet with StdOrder, RunOrder and factor columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_design_dialog = open && !run;
        if run {
            self.run_design_dialog();
        }
    }
    pub fn open_box_behnken_dialog(&mut self) {
        self.design_factors = "A,B,C".to_owned();
        self.design_centers = 3;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_bb_dialog = true;
    }
    pub fn run_box_behnken_dialog(&mut self) {
        let factors: Vec<String> = self
            .design_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.len() < 3 {
            self.set_error("A Box-Behnken design needs at least three factors");
            return;
        }
        let df = match fp_core::doe::box_behnken_design(
            &factors,
            self.design_centers,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Box-Behnken design".to_owned());
        self.append_session(&format!(
            "Box-Behnken design generated: {rows} runs, factors {}.\n",
            factors.join(", ")
        ));
        self.show_bb_dialog = false;
    }
    pub(super) fn box_behnken_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_bb_dialog {
            return;
        }
        let mut open = self.show_bb_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Box-Behnken Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.design_factors)
                            .desired_width(220.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Three to seven factors; every pair is measured at the four corners of a square.");
                ui.horizontal(|ui| {
                    let label = ui.help_label("Center points:");
                    ui.add(egui::DragValue::new(&mut self.design_centers).range(1..=50))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.small("This replaces the current worksheet with StdOrder, RunOrder and factor columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_bb_dialog = open && !run;
        if run {
            self.run_box_behnken_dialog();
        }
    }
    pub fn open_pb_dialog(&mut self) {
        self.design_runs = 12;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_pb_dialog = true;
    }
    pub fn run_pb_dialog(&mut self) {
        let df = match fp_core::doe::plackett_burman_design(
            self.design_runs,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Plackett-Burman design".to_owned());
        self.append_session(&format!(
            "Plackett-Burman design generated: {rows} runs, {} factors.\n",
            rows - 1
        ));
        self.show_pb_dialog = false;
    }
    pub(super) fn pb_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pb_dialog {
            return;
        }
        let mut open = self.show_pb_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Plackett-Burman Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Runs:");
                    ui.add(egui::DragValue::new(&mut self.design_runs).range(8..=48))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("8, 12, 20, 24 or 32");
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("A screening design with runs − 1 orthogonal factors.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_pb_dialog = open && !run;
        if run {
            self.run_pb_dialog();
        }
    }
    pub fn open_taguchi_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.taguchi_response = self
            .selected
            .clone()
            .unwrap_or_else(|| names.last().cloned().unwrap_or_default());
        self.taguchi_factors = names
            .iter()
            .filter(|name| **name != self.taguchi_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.taguchi_quality = fp_core::taguchi::Quality::Smaller;
        self.show_taguchi_dialog = true;
    }
    pub fn run_taguchi_dialog(&mut self) {
        if self.taguchi_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .taguchi_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.is_empty() {
            self.set_error("List at least one factor column");
            return;
        }
        if factors.contains(&self.taguchi_response) {
            self.set_error("The response cannot also be a factor");
            return;
        }
        let spec =
            AnalysisSpec::taguchi(self.taguchi_response.clone(), factors, self.taguchi_quality);
        self.execute_spec(spec, format!("Taguchi: {}", self.taguchi_response));
        self.show_taguchi_dialog = false;
    }
    pub(super) fn taguchi_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_taguchi_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_taguchi_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Taguchi Analysis"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("taguchi_response")
                        .selected_text(if self.taguchi_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.taguchi_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.taguchi_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.taguchi_factors)
                            .desired_width(240.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Quality:");
                    for (quality, label) in [
                        (fp_core::taguchi::Quality::Smaller, "Smaller is better"),
                        (fp_core::taguchi::Quality::Larger, "Larger is better"),
                        (fp_core::taguchi::Quality::Nominal, "Nominal is best"),
                    ] {
                        ui.selectable_value(&mut self.taguchi_quality, quality, label);
                    }
                });
                ui.small("Factor columns hold the level numbers 1, 2, …");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_taguchi_dialog = open && !run;
        if run {
            self.run_taguchi_dialog();
        }
    }
    pub fn open_taguchi_design_dialog(&mut self) {
        self.taguchi_runs = 8;
        self.taguchi_design_factors = 3;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_taguchi_design_dialog = true;
    }
    pub fn run_taguchi_design_dialog(&mut self) {
        let df = match fp_core::taguchi::design(
            self.taguchi_runs,
            self.taguchi_design_factors,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Taguchi design".to_owned());
        self.append_session(&format!(
            "Taguchi design generated: L{rows} with {} factor(s).\n",
            self.taguchi_design_factors
        ));
        self.show_taguchi_design_dialog = false;
    }
    pub(super) fn taguchi_design_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_taguchi_design_dialog {
            return;
        }
        let mut open = self.show_taguchi_design_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Taguchi Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.help_label("Array:");
                    for runs in [8usize, 9, 12, 16, 18, 27] {
                        ui.selectable_value(&mut self.taguchi_runs, runs, format!("L{runs}"));
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(egui::DragValue::new(&mut self.taguchi_design_factors).range(1..=15))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Levels are written as 1, 2, … and analysed by the Taguchi method.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_taguchi_design_dialog = open && !run;
        if run {
            self.run_taguchi_design_dialog();
        }
    }
    pub fn open_rsm_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let numeric: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.len() < 3 {
            self.set_error("Response-surface analysis needs a response and at least two factors");
            return;
        }
        self.rsm_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.rsm_factors = numeric
            .into_iter()
            .filter(|name| *name != self.rsm_response)
            .collect();
        self.show_rsm_dialog = true;
    }
    pub fn run_rsm_dialog(&mut self) {
        if self.rsm_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .rsm_factors
            .iter()
            .filter(|name| **name != self.rsm_response)
            .cloned()
            .collect();
        if factors.len() < 2 {
            self.set_error("Select at least two factors");
            return;
        }
        let spec = AnalysisSpec::response_surface(self.rsm_response.clone(), factors)
            .with_rsm_optimization(self.rsm_optimize)
            .with_rsm_contours(self.rsm_contours)
            .with_rsm_steepest_ascent(self.rsm_steepest_ascent);
        self.execute_spec(spec, format!("Response surface: {}", self.rsm_response));
        self.show_rsm_dialog = false;
    }
    pub(super) fn rsm_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_rsm_dialog {
            return;
        }
        let numeric: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_rsm_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Response Surface"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("rsm_response")
                        .selected_text(if self.rsm_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.rsm_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.rsm_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Factors (coded):");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.rsm_response {
                                continue;
                            }
                            let mut selected = self.rsm_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.rsm_factors.push(name.clone());
                                } else {
                                    self.rsm_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.help_checkbox(&mut self.rsm_optimize, "Stationary point and eigenvalues");
                ui.help_checkbox(&mut self.rsm_contours, "Contour plot (two factors)");
                ui.help_checkbox(&mut self.rsm_steepest_ascent, "Steepest ascent path");
                ui.small("Full quadratic model: main effects, two-way interactions and squares.");
                ui.small("Use a central composite design or another design with residual df.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_rsm_dialog = open && !run;
        if run {
            self.run_rsm_dialog();
        }
    }
    pub fn open_desirability_dialog(&mut self) {
        self.desirability_factors.clear();
        self.desirability_responses.clear();
        self.show_desirability_dialog = true;
    }
    pub fn run_desirability_dialog(&mut self) {
        if self.desirability_factors.len() < 2 {
            self.set_error("Select at least two factor columns");
            return;
        }
        if self.desirability_responses.is_empty() {
            self.set_error("Add at least one response");
            return;
        }
        let parse = |text: &str, what: &str| -> Result<f64, String> {
            text.trim()
                .parse::<f64>()
                .map_err(|_| format!("{what} must be a number"))
        };
        let mut responses = Vec::with_capacity(self.desirability_responses.len());
        for row in &self.desirability_responses {
            let low = match parse(&row.low, "Low") {
                Ok(value) => value,
                Err(message) => return self.set_error(message),
            };
            let high = match parse(&row.high, "High") {
                Ok(value) => value,
                Err(message) => return self.set_error(message),
            };
            let target = if row.goal == Goal::Target {
                match parse(&row.target, "Target") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            } else {
                0.5 * (low + high)
            };
            let weight = if row.weight.trim().is_empty() {
                1.0
            } else {
                match parse(&row.weight, "Weight") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            };
            let importance = if row.importance.trim().is_empty() {
                1.0
            } else {
                match parse(&row.importance, "Importance") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            };
            responses.push(fp_core::desirability::Response {
                column: row.column.clone(),
                goal: row.goal,
                low,
                target,
                high,
                weight,
                importance,
            });
        }
        let spec = AnalysisSpec::desirability(self.desirability_factors.clone(), responses);
        self.execute_spec(spec, "Response Optimizer".to_owned());
        self.show_desirability_dialog = false;
    }
    pub(super) fn desirability_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_desirability_dialog {
            return;
        }
        let numeric: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_desirability_dialog;
        let mut run = false;
        let mut add: Option<String> = None;
        let mut remove: Option<usize> = None;
        dialog_window(self.catalog().label("Response Optimizer"))
            .open(&mut open)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.help_label("Factors (coded −1/+1):");
                egui::ScrollArea::vertical()
                    .max_height(90.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            let mut selected = self.desirability_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.desirability_factors.push(name.clone());
                                } else {
                                    self.desirability_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let label = ui.help_label("Add response:");
                    egui::ComboBox::from_id_salt("desirability_add")
                        .selected_text("—")
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                if ui.selectable_label(false, format!("Add {name}")).clicked() {
                                    add = Some(name.clone());
                                }
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                for index in 0..self.desirability_responses.len() {
                    let row = &mut self.desirability_responses[index];
                    let column = row.column.clone();
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(&column);
                        let label = ui.help_label(format!("Goal ({column}):"));
                        egui::ComboBox::from_id_salt(("desirability_goal", index))
                            .selected_text(row.goal.name())
                            .show_ui(ui, |ui| {
                                for goal in [Goal::Maximize, Goal::Minimize, Goal::Target] {
                                    ui.selectable_value(&mut row.goal, goal, goal.name());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                        if ui.help_button(format!("Remove {column}")).clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (caption, value) in [
                            ("Low", &mut row.low),
                            ("Target", &mut row.target),
                            ("High", &mut row.high),
                        ] {
                            let label = ui.help_label(format!("{caption} ({column}):"));
                            ui.text_edit_singleline(value)
                                .labelled_by(label.id)
                                .hover_from(&label);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (caption, value) in [
                            ("Weight", &mut row.weight),
                            ("Importance", &mut row.importance),
                        ] {
                            let label = ui.help_label(format!("{caption} ({column}):"));
                            ui.text_edit_singleline(value)
                                .labelled_by(label.id)
                                .hover_from(&label);
                        }
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        if let Some(column) = add {
            let (low, high) = self
                .data
                .as_ref()
                .and_then(|df| df.column(&column).ok())
                .and_then(|data| {
                    let sample = data.sample(fp_core::data::MissingPolicy::Drop).ok()?;
                    Some((sample.min().ok()?, sample.max().ok()?))
                })
                .unwrap_or((0.0, 1.0));
            let (low, high) = if low < high {
                (low, high)
            } else {
                (low - 1.0, high + 1.0)
            };
            self.desirability_responses.push(DesirabilityRow {
                column,
                goal: Goal::Maximize,
                low: format!("{low}"),
                target: format!("{}", 0.5 * (low + high)),
                high: format!("{high}"),
                weight: "1".to_owned(),
                importance: "1".to_owned(),
            });
        }
        if let Some(index) = remove {
            self.desirability_responses.remove(index);
        }
        self.show_desirability_dialog = open && !run;
        if run {
            self.run_desirability_dialog();
        }
    }
    pub fn open_step_stress_dialog(&mut self) {
        let numeric: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.ss_duration = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.ss_stress = numeric
            .iter()
            .find(|name| **name != self.ss_duration)
            .cloned()
            .unwrap_or_default();
        self.ss_unit = all
            .iter()
            .find(|name| **name != self.ss_duration && **name != self.ss_stress)
            .cloned()
            .unwrap_or_default();
        self.ss_event = String::new();
        self.show_step_stress_dialog = true;
    }
    pub fn run_step_stress_dialog(&mut self) {
        if self.ss_duration.is_empty()
            || self.ss_stress.is_empty()
            || self.ss_unit.is_empty()
            || self.ss_event.is_empty()
        {
            self.set_error("Select the duration, stress, unit and event columns");
            return;
        }
        let spec = AnalysisSpec::step_stress_alt(
            self.ss_duration.clone(),
            self.ss_stress.clone(),
            self.ss_unit.clone(),
            self.ss_event.clone(),
            self.ss_use_stress,
            self.ss_life,
            self.ss_acceleration,
        );
        self.execute_spec(spec, "Step-Stress ALT".to_owned());
        self.show_step_stress_dialog = false;
    }
    pub(super) fn step_stress_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_step_stress_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_step_stress_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Step-Stress ALT"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Duration:", &mut self.ss_duration),
                    ("Stress:", &mut self.ss_stress),
                    ("Unit:", &mut self.ss_unit),
                    ("Event:", &mut self.ss_event),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Use stress:");
                    ui.add(egui::DragValue::new(&mut self.ss_use_stress).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Life:");
                    for (life, label) in [
                        (fp_core::alt::LifeModel::Weibull, "Weibull"),
                        (fp_core::alt::LifeModel::Exponential, "Exponential"),
                    ] {
                        ui.selectable_value(&mut self.ss_life, life, label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.help_label("Acceleration:");
                    for (model, label) in [
                        (fp_core::alt::AccelerationModel::Arrhenius, "Arrhenius"),
                        (fp_core::alt::AccelerationModel::InversePower, "Power"),
                        (fp_core::alt::AccelerationModel::Eyring, "Eyring"),
                    ] {
                        ui.selectable_value(&mut self.ss_acceleration, model, label);
                    }
                });
                ui.small("One row per step; the failure row is the last step of its unit.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_step_stress_dialog = open && !run;
        if run {
            self.run_step_stress_dialog();
        }
    }
}
