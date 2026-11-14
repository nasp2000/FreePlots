// SPDX-License-Identifier: MIT
//! Statistical dialogs: probability plots, tests, correlation, regression,
//! ANOVA, crosstab, power and the grouped descriptives.

use super::*;

impl FreePlotsApp {
    pub fn open_probability_plot_dialog(&mut self) {
        if self.data.is_none() {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        }
        if let Some(column) = &self.selected {
            self.dialog_columns = vec![column.clone()];
        }
        self.dialog_group = None;
        // The last layout, scales, distribution and confidence are kept in
        // the preferences, so a new session starts where the user left it.
        self.cap_column = self.selected.clone().unwrap_or_default();
        self.cap_lsl.clear();
        self.cap_usl.clear();
        self.cap_target.clear();
        // The capability options are user preferences now.
        self.chart_column = self.selected.clone().unwrap_or_default();
        self.chart_subgroup_column.clear();
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_sizes = None;
        self.t_first = self.selected.clone().unwrap_or_default();
        self.t_second = String::new();
        self.t_null = "0".to_owned();
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        // The confidence is a user preference now.
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        // The confidence is a user preference now.
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.weibull_time = self.selected.clone().unwrap_or_default();
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.exp_time = self.selected.clone().unwrap_or_default();
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.log_time = self.selected.clone().unwrap_or_default();
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.gage_measurement = self.selected.clone().unwrap_or_default();
        self.gage_part = String::new();
        self.gage_operator = String::new();
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.describe_column = self.selected.clone().unwrap_or_default();
        self.describe_by = None;
        self.dialog_distribution = self.distribution;
        self.show_probability_plot_dialog = true;
    }
    pub(super) fn probability_plot_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_probability_plot_dialog {
            return;
        }
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|c| matches!(c.dtype(), DType::Float | DType::Integer))
                    .map(|c| c.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();

        let mut open = self.show_probability_plot_dialog;
        let mut close = false;
        let mut run = false;
        dialog_window(self.catalog().label("Probability Plot"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                ui.help_label("Y variables:");
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 4.0;
                                for name in &columns {
                                    let mut checked = self.dialog_columns.contains(name);
                                    if ui.help_checkbox(&mut checked, name).changed() {
                                        if checked {
                                            self.dialog_columns.push(name.clone());
                                        } else {
                                            self.dialog_columns.retain(|item| item != name);
                                        }
                                    }
                                }
                            });
                    });
                ui.add_space(4.0);

                egui::Grid::new("probability_plot_options")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        let layout_label = ui.help_label("Layout:");
                        let multi = self.dialog_columns.len() > 1;
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(multi, |ui| {
                                egui::ComboBox::from_id_salt("probability_plot_layout")
                                    .width(230.0)
                                    .selected_text(self.dialog_layout.name())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.dialog_layout,
                                            PlotLayout::Separate,
                                            PlotLayout::Separate.name(),
                                        );
                                        ui.selectable_value(
                                            &mut self.dialog_layout,
                                            PlotLayout::Overlay,
                                            PlotLayout::Overlay.name(),
                                        );
                                    })
                                    .response
                                    .labelled_by(layout_label.id).hover_from(&layout_label);
                            });
                            ui.add_enabled_ui(multi, |ui| {
                                ui.horizontal(|ui| {
                                    ui.help_checkbox(&mut self.dialog_same_x, "Same X scale");
                                    ui.help_checkbox(&mut self.dialog_same_y, "Same Y scale");
                                });
                            });
                        });
                        ui.end_row();

                        let group_label = ui.help_label("Group:");
                        let single = self.dialog_columns.len() == 1;
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(single, |ui| {
                                egui::ComboBox::from_id_salt("probability_plot_group")
                                    .width(230.0)
                                    .selected_text(
                                        self.dialog_group
                                            .as_deref()
                                            .filter(|_| single)
                                            .unwrap_or("None"),
                                    )
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.dialog_group, None, "None");
                                        for name in &all_columns {
                                            ui.selectable_value(
                                                &mut self.dialog_group,
                                                Some(name.clone()),
                                                name.as_str(),
                                            );
                                        }
                                    })
                                    .response
                                    .labelled_by(group_label.id).hover_from(&group_label);
                            });
                            if !single {
                                ui.small("Grouping applies to a single Y variable.");
                            }
                        });
                        ui.end_row();

                        let distribution_label = ui.help_label("Distribution:");
                        egui::ComboBox::from_id_salt("probability_plot_distribution")
                            .width(230.0)
                            .selected_text(self.dialog_distribution.name())
                            .show_ui(ui, |ui| {
                                for (kind, name) in [
                                    (DistributionKind::Normal, "Normal"),
                                    (DistributionKind::Lognormal, "Lognormal"),
                                    (DistributionKind::Weibull, "Weibull"),
                                    (DistributionKind::Exponential, "Exponential"),
                                    (DistributionKind::Logistic, "Logistic"),
                                    (DistributionKind::Loglogistic, "Loglogistic"),
                                ] {
                                    ui.selectable_value(&mut self.dialog_distribution, kind, name);
                                }
                            })
                            .response
                            .labelled_by(distribution_label.id).hover_from(&distribution_label);
                        ui.end_row();

                        let display_label = ui.help_label("Display:");
                        egui::ComboBox::from_id_salt("probability_plot_display")
                            .width(230.0)
                            .selected_text(self.dialog_display.name())
                            .show_ui(ui, |ui| {
                                for display in
                                    [PlotDisplay::Both, PlotDisplay::Symbols, PlotDisplay::Fit]
                                {
                                    ui.selectable_value(
                                        &mut self.dialog_display,
                                        display,
                                        display.name(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(display_label.id).hover_from(&display_label);
                        ui.end_row();

                        let band_available = matches!(
                            self.dialog_distribution,
                            DistributionKind::Normal | DistributionKind::Lognormal
                        );
                        let ci_enabled = band_available && self.dialog_display != PlotDisplay::Symbols;
                        let _ = ui.help_label("Confidence:");
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(ci_enabled, |ui| {
                                ui.help_checkbox(&mut self.dialog_ci, "Confidence band");
                                if self.dialog_ci {
                                    ui.horizontal(|ui| {
                                        let label = ui.help_label("Level:");
                                        ui.add(
                                            egui::DragValue::new(&mut self.dialog_ci_level)
                                                .range(1.0..=99.9)
                                                .speed(0.1)
                                                .suffix(" %"),
                                        )
                                        .labelled_by(label.id).hover_from(&label);
                                    });
                                }
                            });
                            if !band_available {
                                ui.small(
                                    "Band not recovered for this distribution (the unrecovered engine layer gap).",
                                );
                            } else if self.dialog_display == PlotDisplay::Symbols {
                                ui.small("CI is suppressed when only symbols are drawn.");
                            }
                        });
                        ui.end_row();
                    });

                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = !self.dialog_columns.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        run = true;
                        close = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_probability_plot_dialog = open && !close;
        if run {
            self.run_probability_plot_dialog();
        }
    }
    pub fn run_probability_plot_dialog(&mut self) {
        if self.dialog_columns.is_empty() {
            self.set_error("Select at least one Y variable");
            return;
        }
        let band_available = matches!(
            self.dialog_distribution,
            DistributionKind::Normal | DistributionKind::Lognormal
        );
        let ci_enabled = band_available && self.dialog_display != PlotDisplay::Symbols;
        let level = (self.dialog_ci_level / 100.0).clamp(0.001, 0.999);
        let confidence = if self.dialog_ci && ci_enabled {
            Some(level)
        } else {
            None
        };
        let mut spec = AnalysisSpec::probability_plot_multi(self.dialog_columns.clone())
            .with_distribution(self.dialog_distribution)
            .with_display(self.dialog_display)
            .with_layout(self.dialog_layout)
            .with_same_scale(self.dialog_same_x, self.dialog_same_y)
            .with_confidence(confidence);
        if self.dialog_columns.len() == 1
            && let Some(group) = &self.dialog_group
        {
            spec = spec.with_group(group.clone());
        }
        self.distribution = self.dialog_distribution;
        let label = format!("Probability plot: {}", spec.column());
        self.show_probability_plot_dialog = false;
        self.execute_spec(spec, label);
    }
    pub fn open_correlation_dialog(&mut self) {
        self.correlation_columns = self.selected.clone().unwrap_or_default();
        self.show_correlation_dialog = true;
    }
    pub fn run_correlation_dialog(&mut self) {
        let columns: Vec<String> = self
            .correlation_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::correlation(columns);
        self.execute_spec(spec, "Correlation Matrix".to_owned());
        self.show_correlation_dialog = false;
    }
    pub(super) fn correlation_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_correlation_dialog {
            return;
        }
        let mut open = self.show_correlation_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Correlation Matrix"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.correlation_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Comma-separated column names; every pair is correlated.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_correlation_dialog = open && !run;
        if run {
            self.run_correlation_dialog();
        }
    }
    pub fn open_curve_dialog(&mut self) {
        self.curve_x = self.selected.clone().unwrap_or_default();
        self.curve_y = String::new();
        self.show_curve_dialog = true;
    }
    pub fn run_curve_dialog(&mut self) {
        if self.curve_x.is_empty() {
            self.set_error("Select the X column");
            return;
        }
        if self.curve_y.is_empty() {
            self.set_error("Select the Y column");
            return;
        }
        if self.curve_x == self.curve_y {
            self.set_error("Select two different columns");
            return;
        }
        let spec =
            AnalysisSpec::curve_fit(self.curve_x.clone(), self.curve_y.clone(), self.curve_model)
                .with_curve_fit_confidence((self.curve_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!("Curve Fit: {} vs {}", self.curve_y, self.curve_x);
        self.execute_spec(spec, label);
        self.show_curve_dialog = false;
    }
    pub(super) fn curve_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_curve_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_curve_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Curve Fit"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("X:");
                    egui::ComboBox::from_id_salt("curve_x")
                        .selected_text(if self.curve_x.is_empty() {
                            "—".to_owned()
                        } else {
                            self.curve_x.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.curve_x, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Y:");
                    egui::ComboBox::from_id_salt("curve_y")
                        .selected_text(if self.curve_y.is_empty() {
                            "—".to_owned()
                        } else {
                            self.curve_y.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.curve_y, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Model:");
                    egui::ComboBox::from_id_salt("curve_model")
                        .selected_text(self.curve_model.name())
                        .show_ui(ui, |ui| {
                            for model in [
                                CurveModel::Polynomial(1),
                                CurveModel::Polynomial(2),
                                CurveModel::Polynomial(3),
                                CurveModel::Polynomial(4),
                                CurveModel::Polynomial(5),
                                CurveModel::Exponential,
                                CurveModel::Power,
                                CurveModel::Logistic,
                            ] {
                                ui.selectable_value(
                                    &mut self.curve_model,
                                    model,
                                    model.name().as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.curve_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "The exponential and the power model need positive values; they are fitted in \
                     the log scale.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_curve_dialog = open && !run;
        if run {
            self.run_curve_dialog();
        }
    }
    pub fn open_regression_dialog(&mut self) {
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
        let all: Vec<String> = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.is_empty() || all.len() < 2 {
            self.set_error("Regression needs a numeric response and at least one predictor");
            return;
        }
        self.reg_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.reg_predictors = all
            .into_iter()
            .filter(|name| *name != self.reg_response)
            .collect();
        self.show_reg_dialog = true;
    }
    pub fn run_regression_dialog(&mut self) {
        if self.reg_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let predictors: Vec<String> = self
            .reg_predictors
            .iter()
            .filter(|name| **name != self.reg_response)
            .cloned()
            .collect();
        if predictors.is_empty() {
            self.set_error("Select at least one predictor");
            return;
        }
        let spec = AnalysisSpec::regression(self.reg_response.clone(), predictors)
            .with_regression_confidence((self.reg_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Regression: {}", self.reg_response));
        self.show_reg_dialog = false;
    }
    /// Opens the prediction dialog with the regression model preselected.
    pub fn open_predict_dialog(&mut self) {
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
            self.set_error("Prediction needs a numeric response and at least one predictor");
            return;
        }
        self.predict_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.predict_predictors = numeric
            .into_iter()
            .filter(|name| *name != self.predict_response)
            .collect();
        self.predict_points.clear();
        self.show_predict_dialog = true;
    }

    /// Predicts the response at the points of the dialog.
    pub fn run_predict_dialog(&mut self) {
        if self.predict_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let predictors: Vec<String> = self
            .predict_predictors
            .iter()
            .filter(|name| **name != self.predict_response)
            .cloned()
            .collect();
        if predictors.is_empty() {
            self.set_error("Select at least one predictor");
            return;
        }
        let points = match parse_prediction_points(&self.predict_points) {
            Ok(points) => points,
            Err(error) => {
                self.set_error(&error);
                return;
            }
        };
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::regression::predict_points(
            &df,
            &self.predict_response,
            &predictors,
            self.predict_confidence / 100.0,
            &points,
        ) {
            Ok(result) => {
                self.append_session(&format!("{}\n", result.to_session_text()));
                self.status = format!("Predicted {} point(s)", points.len());
                self.show_predict_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Prediction dialog: the model and the new points.
    pub(super) fn predict_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_predict_dialog {
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
        let mut open = self.show_predict_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Predict"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("predict_response")
                        .selected_text(if self.predict_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.predict_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.predict_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Predictors:");
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.predict_response {
                                continue;
                            }
                            let mut selected = self.predict_predictors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.predict_predictors.push(name.clone());
                                } else {
                                    self.predict_predictors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.predict_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                let points_label = ui.help_label("New points:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.predict_points)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY)
                        .hint_text("one row per point, values in predictor order"),
                )
                .labelled_by(points_label.id)
                .hover_from(&points_label);
                ui.small(
                    "One row per new point, with one number per predictor in the order of the \
                     list, separated by commas or spaces. The model is fitted to the worksheet \
                     when the command runs.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About prediction…"))
                        .clicked()
                    {
                        self.show_predict_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.predict_response.is_empty()
                                && !self.predict_points.trim().is_empty(),
                            egui::Button::new(self.catalog().label("OK")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_predict_dialog = open && !run && !cancel;
        if run {
            self.run_predict_dialog();
        }
    }

    /// Help page of the prediction dialog.
    pub(super) fn predict_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_predict_help {
            return;
        }
        let mut open = self.show_predict_help;
        let mut close = false;
        dialog_window(self.catalog().label("Prediction help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "Predict evaluates a fitted linear model at new predictor values. It reports \
                     the fitted response, the confidence interval for the mean response and the \
                     prediction interval for a single new observation.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose the response and at least one numeric predictor — the same model the \
                     regression report describes. Enter one or more new points, one row per \
                     point, with one value per predictor in the order of the list, separated by \
                     commas or spaces. The model is fitted to every complete row when the command \
                     runs; rows with a missing value are excluded listwise and counted.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A model of weight on height and pressure predicts the weight at height 12 \
                     and pressure 105. Enter 12, 105 as one row. The report gives the fitted \
                     weight and both intervals; values far outside the fitted range are \
                     extrapolations.",
                ));
                egui::Grid::new("predict_help_example")
                    .num_columns(3)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("weight");
                        ui.strong("height");
                        ui.strong("pressure");
                        ui.end_row();
                        for (weight, height, pressure) in [
                            ("12.1", "10", "100"),
                            ("12.4", "11", "102"),
                            ("12.9", "12", "105"),
                        ] {
                            ui.monospace(weight);
                            ui.monospace(height);
                            ui.monospace(pressure);
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_predict_help = open && !close;
    }

    /// Opens the graphical summary dialog with the selected column.
    pub fn open_gsummary_dialog(&mut self) {
        self.gsummary_column = self.selected.clone().unwrap_or_default();
        self.show_gsummary_dialog = true;
    }

    /// Runs the graphical summary of the dialog column.
    pub fn run_gsummary_dialog(&mut self) {
        if self.gsummary_column.is_empty() {
            self.set_error("Select the column to summarise");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::summary::graphical_summary(&df, &self.gsummary_column) {
            Ok(output) => {
                let label = format!("Graphical summary: {}", self.gsummary_column);
                self.apply_output(output, label);
                self.show_gsummary_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the store descriptive statistics dialog.
    pub fn open_store_dialog(&mut self) {
        let selected = self.selected.clone().unwrap_or_default();
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
        self.store_columns = if numeric.contains(&selected) {
            vec![selected]
        } else {
            numeric.into_iter().take(1).collect()
        };
        self.show_store_dialog = true;
    }

    /// Writes the selected descriptive statistics into new columns.
    pub fn run_store_dialog(&mut self) {
        if self.store_columns.is_empty() {
            self.set_error("Select at least one variable");
            return;
        }
        if self.store_stats.is_empty() {
            self.set_error("Select at least one statistic");
            return;
        }
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::summary::store_descriptive_statistics(
            &mut df,
            &self.store_columns,
            &self.store_stats,
            self.store_confidence / 100.0,
        ) {
            Ok(added) => {
                let names = added.join(", ");
                self.set_data(df, "Stored statistics".to_owned());
                self.append_session(&format!("Descriptive statistics stored in: {names}\n"));
                self.status = format!("Stored {} column(s)", added.len());
                self.show_store_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Graphical summary dialog.
    pub(super) fn gsummary_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_gsummary_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_gsummary_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Graphical Summary"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("gsummary_column")
                        .selected_text(if self.gsummary_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gsummary_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.gsummary_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "Histogram, boxplot and the 95% confidence intervals of the mean, median and \
                     standard deviation.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About these commands…"))
                        .clicked()
                    {
                        self.show_summary_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.gsummary_column.is_empty(),
                            egui::Button::new(self.catalog().label("OK")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_gsummary_dialog = open && !run && !cancel;
        if run {
            self.run_gsummary_dialog();
        }
    }

    /// Store descriptive statistics dialog.
    pub(super) fn store_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_store_dialog {
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
        let mut open = self.show_store_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Store Descriptive Statistics"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.help_label("Variables:");
                for name in &numeric {
                    let mut selected = self.store_columns.contains(name);
                    if ui.help_checkbox(&mut selected, name).changed() {
                        if selected {
                            self.store_columns.push(name.clone());
                        } else {
                            self.store_columns.retain(|value| value != name);
                        }
                    }
                }
                ui.help_label("Statistics:");
                let stats = [
                    fp_core::summary::StoreStat::N,
                    fp_core::summary::StoreStat::NMissing,
                    fp_core::summary::StoreStat::Mean,
                    fp_core::summary::StoreStat::SeMean,
                    fp_core::summary::StoreStat::StDev,
                    fp_core::summary::StoreStat::Minimum,
                    fp_core::summary::StoreStat::Q1,
                    fp_core::summary::StoreStat::Median,
                    fp_core::summary::StoreStat::Q3,
                    fp_core::summary::StoreStat::Maximum,
                    fp_core::summary::StoreStat::Iqr,
                    fp_core::summary::StoreStat::CiMeanLower,
                    fp_core::summary::StoreStat::CiMeanUpper,
                    fp_core::summary::StoreStat::CiMedianLower,
                    fp_core::summary::StoreStat::CiMedianUpper,
                    fp_core::summary::StoreStat::CiSdLower,
                    fp_core::summary::StoreStat::CiSdUpper,
                ];
                for row in 0..stats.len().div_ceil(2) {
                    ui.horizontal(|ui| {
                        for column in 0..2 {
                            let Some(stat) = stats.get(row * 2 + column).copied() else {
                                continue;
                            };
                            let mut selected = self.store_stats.contains(&stat);
                            if ui.help_checkbox(&mut selected, stat.name()).changed() {
                                if selected {
                                    self.store_stats.push(stat);
                                } else {
                                    self.store_stats.retain(|value| *value != stat);
                                }
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.store_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "The table is appended after the existing rows: a Variable column and one \
                     column per statistic.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About these commands…"))
                        .clicked()
                    {
                        self.show_summary_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.store_columns.is_empty() && !self.store_stats.is_empty(),
                            egui::Button::new(self.catalog().label("OK")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_store_dialog = open && !run && !cancel;
        if run {
            self.run_store_dialog();
        }
    }

    /// Help page of the graphical summary and the stored statistics.
    pub(super) fn summary_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_summary_help {
            return;
        }
        let mut open = self.show_summary_help;
        let mut close = false;
        dialog_window(self.catalog().label("Summary help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The graphical summary stacks a histogram, a boxplot and the 95% confidence \
                     intervals of the mean, the median and the standard deviation. Store \
                     Descriptive Statistics writes the selected statistics as new worksheet \
                     columns, one row per variable.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose one numeric column for the summary. For the store command, select one \
                     or more numeric columns and the statistics: the table gets a Variable column \
                     and one column per statistic at the end of the worksheet, below the existing \
                     rows. Missing values are excluded and counted.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "Five weights 10, 11, 12, 13 and 14 give a mean of 12 and a standard \
                     deviation of 1.581. Storing N, Mean and StDev adds three columns with one \
                     row for the variable.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_summary_help = open && !close;
    }

    /// Opens the covariance dialog with the selected column.
    pub fn open_covariance_dialog(&mut self) {
        self.covariance_columns = self.selected.clone().unwrap_or_default();
        self.show_covariance_dialog = true;
    }

    /// Runs the covariance matrix of the dialog columns.
    pub fn run_covariance_dialog(&mut self) {
        let columns: Vec<String> = self
            .covariance_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::correlation::covariance(&df, &columns) {
            Ok(result) => {
                self.apply_output(
                    fp_core::workflow::AnalysisOutput {
                        results: vec![result],
                        plot: None,
                    },
                    "Covariance".to_owned(),
                );
                self.show_covariance_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the tally dialog with the selected column.
    pub fn open_tally_dialog(&mut self) {
        self.tally_columns = self.selected.clone().into_iter().collect();
        self.show_tally_dialog = true;
    }

    /// Runs the tally of the dialog columns.
    pub fn run_tally_dialog(&mut self) {
        if self.tally_columns.is_empty() {
            self.set_error("Select at least one column");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::tally::tally(&df, &self.tally_columns) {
            Ok(result) => {
                self.apply_output(
                    fp_core::workflow::AnalysisOutput {
                        results: vec![result],
                        plot: None,
                    },
                    "Tally".to_owned(),
                );
                self.show_tally_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Covariance dialog.
    pub(super) fn covariance_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_covariance_dialog {
            return;
        }
        let mut open = self.show_covariance_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Covariance"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.covariance_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "Two or more numeric columns, separated by commas. Each cell uses the rows \
                     complete in both columns and divides by n − 1.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About these commands…"))
                        .clicked()
                    {
                        self.show_cov_tally_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.covariance_columns.trim().is_empty(),
                            egui::Button::new(self.catalog().label("OK")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_covariance_dialog = open && !run && !cancel;
        if run {
            self.run_covariance_dialog();
        }
    }

    /// Tally dialog.
    pub(super) fn tally_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_tally_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_tally_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Tally"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.help_label("Columns:");
                for name in &names {
                    let mut selected = self.tally_columns.contains(name);
                    if ui.help_checkbox(&mut selected, name).changed() {
                        if selected {
                            self.tally_columns.push(name.clone());
                        } else {
                            self.tally_columns.retain(|value| value != name);
                        }
                    }
                }
                ui.small(
                    "Counts of the unique values of each column, with percent and cumulative \
                     columns. Missing values are excluded.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About these commands…"))
                        .clicked()
                    {
                        self.show_cov_tally_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.tally_columns.is_empty(),
                            egui::Button::new(self.catalog().label("OK")),
                        )
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_tally_dialog = open && !run && !cancel;
        if run {
            self.run_tally_dialog();
        }
    }

    /// Help page of the covariance and tally commands.
    pub(super) fn cov_tally_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_cov_tally_help {
            return;
        }
        let mut open = self.show_cov_tally_help;
        let mut close = false;
        dialog_window(self.catalog().label("Covariance and tally help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The covariance matrix measures how pairs of numeric columns vary together; \
                     the tally counts the unique values of one or more columns with their \
                     percentages.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose at least two numeric columns for the covariance matrix, separated by \
                     commas; each cell uses the rows that are complete in both columns. Tally \
                     accepts numeric, text and date columns; each selected column gets a table \
                     with the count, percent, cumulative count and cumulative percent. Missing \
                     values are excluded and counted.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "x = 1, 2, 3, 4 and y = 2, 4, 6, 8 give Var(x) = 1.667, Cov(x, y) = 3.333 \
                     and Var(y) = 6.667. A grade column A, A, B, A, B tallies A with 3 (60 %) \
                     and B with 2 (40 %).",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_cov_tally_help = open && !close;
    }

    pub(super) fn regression_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_reg_dialog {
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
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_reg_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("reg_response")
                        .selected_text(if self.reg_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.reg_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.reg_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.help_label("Predictors:");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &all {
                            if *name == self.reg_response {
                                continue;
                            }
                            let mut selected = self.reg_predictors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.reg_predictors.push(name.clone());
                                } else {
                                    self.reg_predictors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.reg_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Ordinary least squares with intercept; categorical predictors use dummy coding.");
                ui.small("Rows missing the response or any selected predictor are removed.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_reg_dialog = open && !run;
        if run {
            self.run_regression_dialog();
        }
    }
    /// Opens the best-subsets dialog with the selected response.
    pub fn open_best_subsets_dialog(&mut self) {
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
            self.set_error("Best subsets needs a numeric response and at least one predictor");
            return;
        }
        self.bs_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.bs_predictors = numeric
            .into_iter()
            .filter(|name| *name != self.bs_response)
            .collect();
        self.show_best_subsets_dialog = true;
    }

    /// Runs the best-subsets enumeration of the dialog.
    pub fn run_best_subsets_dialog(&mut self) {
        if self.bs_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let predictors: Vec<String> = self
            .bs_predictors
            .iter()
            .filter(|name| **name != self.bs_response)
            .cloned()
            .collect();
        if predictors.is_empty() {
            self.set_error("Select at least one predictor");
            return;
        }
        if predictors.len() > fp_core::regression::MAX_BEST_SUBSETS_PREDICTORS {
            self.set_error(format!(
                "Best subsets enumerates at most {} predictors",
                fp_core::regression::MAX_BEST_SUBSETS_PREDICTORS
            ));
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::regression::best_subsets(&df, &self.bs_response, &predictors) {
            Ok(output) => {
                let label = format!("Best subsets: {}", self.bs_response);
                self.apply_output(output, label);
                self.show_best_subsets_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Best subsets regression dialog.
    pub(super) fn best_subsets_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_best_subsets_dialog {
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
        let mut open = self.show_best_subsets_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Best Subsets Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("bs_response")
                        .selected_text(if self.bs_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.bs_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.bs_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Predictors:");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.bs_response {
                                continue;
                            }
                            let mut selected = self.bs_predictors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.bs_predictors.push(name.clone());
                                } else {
                                    self.bs_predictors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.small(
                    "Every non-empty subset of the predictors is fitted; the table lists the two \
                     best per size by R² and the best by R², adjusted R², Cp and s.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About best subsets…"))
                        .clicked()
                    {
                        self.show_best_subsets_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Run")).clicked() {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_best_subsets_dialog = open && !run && !cancel;
        if run {
            self.run_best_subsets_dialog();
        }
    }

    /// Help page of the best-subsets regression.
    pub(super) fn best_subsets_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_best_subsets_help {
            return;
        }
        let mut open = self.show_best_subsets_help;
        let mut close = false;
        dialog_window(self.catalog().label("Best subsets regression help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "Best subsets fits every non-empty subset of the candidate predictors and \
                     ranks the models by R², adjusted R², Mallows' Cp and the residual standard \
                     error s. It is a screening step before fitting a final regression.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose one numeric response and at least one numeric predictor, up to 12. \
                     Rows with a missing value in the response or in any candidate predictor are \
                     removed listwise, so every model uses the same observations. A subset with \
                     collinear predictors is skipped; a collinear full model is an error because \
                     it is the Cp reference.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "With a response built from x1 and x2 and an unrelated x3, the best adjusted \
                     R² and the smallest Cp both choose the model with x1 and x2; its Cp is close \
                     to 3, the number of parameters. Adding x3 raises R² only slightly and lowers \
                     the adjusted R².",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_best_subsets_help = open && !close;
    }

    pub fn open_stepwise_dialog(&mut self) {
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
        if numeric.is_empty() || numeric.len() < 2 {
            self.set_error("Stepwise regression needs a numeric response and at least one other numeric column");
            return;
        }
        self.step_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.step_candidates = numeric
            .into_iter()
            .filter(|name| *name != self.step_response)
            .collect();
        // The method, criterion, alphas and step limit are preferences now.
        self.show_step_dialog = true;
    }
    pub fn run_stepwise_dialog(&mut self) {
        if self.step_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let candidates: Vec<String> = self
            .step_candidates
            .iter()
            .filter(|name| **name != self.step_response)
            .cloned()
            .collect();
        if candidates.is_empty() {
            self.set_error("Select at least one candidate predictor");
            return;
        }
        let spec = AnalysisSpec::stepwise_regression(self.step_response.clone(), candidates)
            .with_stepwise_method(self.step_method)
            .with_stepwise_criterion(self.step_criterion)
            .with_stepwise_alphas(self.step_alpha_enter, self.step_alpha_remove)
            .with_stepwise_max_steps(self.step_max_steps)
            .with_regression_confidence((self.step_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Stepwise Regression: {}", self.step_response));
        self.show_step_dialog = false;
    }
    pub(super) fn stepwise_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_step_dialog {
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
        let mut open = self.show_step_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stepwise Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("step_response")
                        .selected_text(if self.step_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.step_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.step_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Candidate predictors:");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.step_response {
                                continue;
                            }
                            let mut selected = self.step_candidates.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.step_candidates.push(name.clone());
                                } else {
                                    self.step_candidates.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.help_label("Method:");
                    ui.radio_value(&mut self.step_method, StepMethod::Forward, "Forward")
                        .on_hover_text(checkbox_help("Forward"));
                    ui.radio_value(&mut self.step_method, StepMethod::Backward, "Backward")
                        .on_hover_text(checkbox_help("Backward"));
                    ui.radio_value(&mut self.step_method, StepMethod::Stepwise, "Stepwise")
                        .on_hover_text(checkbox_help("Stepwise"));
                });
                ui.horizontal(|ui| {
                    ui.help_label("Criterion:");
                    ui.radio_value(&mut self.step_criterion, StepCriterion::PValue, "p-value")
                        .on_hover_text(checkbox_help("p-value"));
                    ui.radio_value(&mut self.step_criterion, StepCriterion::Aic, "AIC")
                        .on_hover_text(checkbox_help("AIC"));
                    ui.radio_value(&mut self.step_criterion, StepCriterion::Bic, "BIC")
                        .on_hover_text(checkbox_help("BIC"));
                });
                ui.add_enabled_ui(self.step_criterion == StepCriterion::PValue, |ui| {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Entry alpha:");
                        ui.add(
                            egui::DragValue::new(&mut self.step_alpha_enter)
                                .range(0.001..=0.5)
                                .speed(0.01)
                                .max_decimals(3),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Removal alpha:");
                        ui.add(
                            egui::DragValue::new(&mut self.step_alpha_remove)
                                .range(0.001..=0.5)
                                .speed(0.01)
                                .max_decimals(3),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Max steps:");
                    ui.add(
                        egui::DragValue::new(&mut self.step_max_steps)
                            .range(1..=500)
                            .speed(1.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.step_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Numeric candidates only; every candidate model uses the same complete rows.",
                );
                ui.small("Selected coefficients are ordinary least squares on the final model.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_step_dialog = open && !run;
        if run {
            self.run_stepwise_dialog();
        }
    }
    /// Opens the general linear model dialog with the selected response.
    pub fn open_glm_dialog(&mut self) {
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
        let all: Vec<String> = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.is_empty() {
            self.set_error("The general linear model needs a numeric response");
            return;
        }
        self.glm_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.glm_factors = all
            .into_iter()
            .filter(|name| *name != self.glm_response && !numeric.contains(name))
            .collect();
        self.glm_covariates = numeric
            .into_iter()
            .filter(|name| *name != self.glm_response)
            .collect();
        self.glm_interactions.clear();
        self.show_glm_dialog = true;
    }

    /// Runs the general linear model of the dialog.
    pub fn run_glm_dialog(&mut self) {
        if self.glm_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        if self.glm_factors.is_empty() && self.glm_covariates.is_empty() {
            self.set_error("Select at least one factor or covariate");
            return;
        }
        let mut interactions = Vec::new();
        for part in self.glm_interactions.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let sides: Vec<&str> = part.split('*').map(str::trim).collect();
            if sides.len() != 2 || sides[0].is_empty() || sides[1].is_empty() {
                self.set_error("Write each interaction as A*B, separated by commas");
                return;
            }
            interactions.push((sides[0].to_owned(), sides[1].to_owned()));
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let options = fp_core::glm::GlmOptions {
            factors: self
                .glm_factors
                .iter()
                .filter(|name| **name != self.glm_response)
                .cloned()
                .collect(),
            covariates: self
                .glm_covariates
                .iter()
                .filter(|name| **name != self.glm_response)
                .cloned()
                .collect(),
            interactions,
        };
        match fp_core::glm::analyze(&df, &self.glm_response, &options) {
            Ok(output) => {
                let label = format!("General linear model: {}", self.glm_response);
                self.apply_output(output, label);
                self.show_glm_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// General linear model dialog.
    pub(super) fn glm_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_glm_dialog {
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
        let categorical: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| !matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_glm_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("General Linear Model"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("glm_response")
                        .selected_text(if self.glm_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.glm_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.glm_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Factors (reference coding):");
                egui::ScrollArea::vertical()
                    .id_salt("glm_factors")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for name in &categorical {
                            if *name == self.glm_response {
                                continue;
                            }
                            let mut selected = self.glm_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.glm_factors.push(name.clone());
                                } else {
                                    self.glm_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.help_label("Covariates (numeric):");
                egui::ScrollArea::vertical()
                    .id_salt("glm_covariates")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.glm_response {
                                continue;
                            }
                            let mut selected = self.glm_covariates.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.glm_covariates.push(name.clone());
                                } else {
                                    self.glm_covariates.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Interactions (A*B):");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.glm_interactions)
                            .desired_width(250.0)
                            .hint_text("A*B, A*x"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Factors use reference coding; interactions are two-way products. Rows missing \
                     the response, a factor label or a covariate are removed listwise.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About the general linear model…"))
                        .clicked()
                    {
                        self.show_glm_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Run")).clicked() {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_glm_dialog = open && !run && !cancel;
        if run {
            self.run_glm_dialog();
        }
    }

    /// Help page of the general linear model.
    pub(super) fn glm_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_glm_help {
            return;
        }
        let mut open = self.show_glm_help;
        let mut close = false;
        dialog_window(self.catalog().label("General linear model help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The general linear model fits one numeric response from categorical \
                     factors, numeric covariates and two-way interactions. Factors use reference \
                     coding and every term gets sequential (Type I) and adjusted (Type III) sums \
                     of squares with an F test.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose the response, the factors and the covariates. Factors are the text or \
                     date columns; covariates are the numeric columns. Write each interaction \
                     as A*B, separated by commas, using names already declared as factors or \
                     covariates. Rows missing the response, a factor label or a covariate are \
                     removed listwise.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A response measured on two machines (factor) at several speeds (covariate), \
                     with the Machine*Speed interaction, gives an ANOVA table with Machine, \
                     Speed and Machine*Speed; the interaction tests whether the speed slopes \
                     differ between the machines.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_glm_help = open && !close;
    }

    pub fn open_anova_dialog(&mut self) {
        self.open_one_way_dialog(OneWayKind::Anova);
    }
    pub fn open_one_way_dialog(&mut self, kind: OneWayKind) {
        self.one_way_kind = kind;
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
        if numeric.is_empty() {
            self.set_error("ANOVA needs a numeric response");
            return;
        }
        self.anova_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.anova_factor = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.anova_response)
            .unwrap_or_default();
        self.show_anova_dialog = true;
    }
    pub fn run_anova_dialog(&mut self) {
        if self.anova_response.is_empty() || self.anova_factor.is_empty() {
            self.set_error("Select the response and factor columns");
            return;
        }
        if self.anova_response == self.anova_factor {
            self.set_error("The response and factor must be different");
            return;
        }
        let confidence = (self.anova_confidence / 100.0).clamp(0.5, 0.999);
        let spec = match self.one_way_kind {
            OneWayKind::Anova => {
                AnalysisSpec::anova(self.anova_response.clone(), self.anova_factor.clone())
                    .with_anova_confidence(confidence)
            }
            OneWayKind::Levene => {
                AnalysisSpec::levene(self.anova_response.clone(), self.anova_factor.clone())
                    .with_variance_center(self.variance_center)
                    .with_levene_confidence(confidence)
            }
            OneWayKind::Bartlett => {
                AnalysisSpec::bartlett(self.anova_response.clone(), self.anova_factor.clone())
            }
        };
        self.execute_spec(
            spec,
            format!(
                "{}: {} by {}",
                self.one_way_kind.name(),
                self.anova_response,
                self.anova_factor
            ),
        );
        self.show_anova_dialog = false;
    }
    pub(super) fn anova_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_anova_dialog {
            return;
        }
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
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
        let mut open = self.show_anova_dialog;
        let mut run = false;
        dialog_window(self.one_way_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Test:");
                    egui::ComboBox::from_id_salt("one_way_kind")
                        .selected_text(self.one_way_kind.name())
                        .show_ui(ui, |ui| {
                            for kind in [OneWayKind::Anova, OneWayKind::Levene, OneWayKind::Bartlett] {
                                ui.selectable_value(&mut self.one_way_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("anova_response")
                        .selected_text(if self.anova_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.anova_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.anova_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factor:");
                    egui::ComboBox::from_id_salt("anova_factor")
                        .selected_text(if self.anova_factor.is_empty() {
                            "—".to_owned()
                        } else {
                            self.anova_factor.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.anova_factor,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                if self.one_way_kind == OneWayKind::Levene {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Centre:");
                        egui::ComboBox::from_id_salt("variance_center")
                            .selected_text(self.variance_center.as_str())
                            .show_ui(ui, |ui| {
                                for center in [VarianceCenter::Mean, VarianceCenter::Median] {
                                    ui.selectable_value(
                                        &mut self.variance_center,
                                        center,
                                        center.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id).hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.anova_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small(match self.one_way_kind {
                    OneWayKind::Anova => {
                        "Groups need at least two observations; pairwise comparisons use Tukey HSD and Bonferroni."
                    }
                    OneWayKind::Levene => {
                        "The spread of each observation is measured against the centre of its \
                         group; the F test compares the groups."
                    }
                    OneWayKind::Bartlett => {
                        "Likelihood-ratio test of equal variances; sensitive to departures from \
                         normality."
                    }
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_anova_dialog = open && !run;
        if run {
            self.run_anova_dialog();
        }
    }
}

/// Parses one new point per non-empty line, values separated by commas,
/// semicolons or spaces.
fn parse_prediction_points(text: &str) -> Result<Vec<Vec<f64>>, String> {
    let mut points = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut values = Vec::new();
        for token in line.split(|character: char| {
            character == ',' || character == ';' || character.is_whitespace()
        }) {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let value = token
                .parse::<f64>()
                .map_err(|_| format!("point {}: invalid number {token}", index + 1))?;
            values.push(value);
        }
        if !values.is_empty() {
            points.push(values);
        }
    }
    if points.is_empty() {
        return Err("Enter at least one new point".to_owned());
    }
    Ok(points)
}
