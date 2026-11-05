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
        self.cap_subgroup = 1;
        self.cap_bootstrap = 0;
        self.cap_box_cox = false;
        self.chart_column = self.selected.clone().unwrap_or_default();
        self.chart_subgroup_column.clear();
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_kind = AttributeChartKind::P;
        self.attr_sizes = None;
        self.attr_size = "100".to_owned();
        self.t_first = self.selected.clone().unwrap_or_default();
        self.t_second = String::new();
        self.t_null = "0".to_owned();
        self.t_alternative = Alternative::TwoSided;
        self.t_pooled = false;
        self.t_confidence = 95.0;
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.test2_success = "1".to_owned();
        self.test2_method = ProportionMethod::Exact;
        self.test2_alternative = Alternative::TwoSided;
        self.test2_confidence = 95.0;
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        // The confidence is a user preference now.
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        // The confidence is a user preference now.
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
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
        self.gage_design = GageDesign::Crossed;
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.doe_main_effects = false;
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.rsm_optimize = false;
        self.rsm_contours = false;
        self.rsm_steepest_ascent = false;
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.scatter_fit = true;
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
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
        self.curve_model = CurveModel::Polynomial(2);
        self.curve_confidence = 95.0;
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
        self.reg_confidence = 95.0;
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
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
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
        self.anova_confidence = 95.0;
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
