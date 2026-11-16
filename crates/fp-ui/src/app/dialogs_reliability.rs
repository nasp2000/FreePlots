// SPDX-License-Identifier: MIT
//! Reliability dialogs: Kaplan-Meier, Weibull, exponential, lognormal and ALT.

use super::*;

impl FreePlotsApp {
    pub fn open_kaplan_meier_dialog(&mut self) {
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
            self.set_error("Kaplan–Meier needs a numeric time column");
            return;
        }
        self.km_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.show_km_dialog = true;
    }
    pub fn run_kaplan_meier_dialog(&mut self) {
        if self.km_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.km_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::kaplan_meier(self.km_time.clone())
            .with_censoring(self.km_censor.clone(), censor_value)
            .with_reliability_confidence((self.km_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Kaplan–Meier: {}", self.km_time));
        self.show_km_dialog = false;
    }
    pub(super) fn kaplan_meier_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_km_dialog {
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
        let mut open = self.show_km_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Kaplan–Meier"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("km_time")
                        .selected_text(if self.km_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.km_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(&mut self.km_time, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("km_censor")
                        .selected_text(self.km_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.km_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.km_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.km_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.km_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Numeric censoring column: the configured value means right-censored.");
                ui.small("Greenwood standard error; normal interval clipped to [0, 1].");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_km_dialog = open && !run;
        if run {
            self.run_kaplan_meier_dialog();
        }
    }
    /// Opens the survival analysis dialog.
    pub fn open_survival_dialog(&mut self) {
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
            self.set_error("The survival analysis needs a numeric time column");
            return;
        }
        self.survival_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.survival_censor = None;
        self.survival_censor_value = "0".to_owned();
        // Default the mission to the median of the time column.
        let mut values: Vec<f64> = df
            .column(&self.survival_time)
            .ok()
            .map(|column| {
                (0..column.len())
                    .filter_map(|row| column.value(row))
                    .filter(|value| value.is_finite())
                    .collect()
            })
            .unwrap_or_default();
        values.sort_by(f64::total_cmp);
        let median = if values.is_empty() {
            1.0
        } else if values.len() % 2 == 1 {
            values[values.len() / 2]
        } else {
            0.5 * (values[values.len() / 2 - 1] + values[values.len() / 2])
        };
        self.survival_mission = format!("{median}");
        self.show_survival_dialog = true;
    }

    /// Runs the survival analysis of the dialog.
    pub fn run_survival_dialog(&mut self) {
        if self.survival_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.survival_censor_value.trim().parse::<f64>() else {
            self.set_error("The censoring value must be a number");
            return;
        };
        let Ok(mission) = self.survival_mission.trim().parse::<f64>() else {
            self.set_error("The mission time must be a number");
            return;
        };
        if mission <= 0.0 {
            self.set_error("The mission time must be positive");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::survival::analyze(
            &df,
            &self.survival_time,
            self.survival_censor.as_deref(),
            censor_value,
            mission,
        ) {
            Ok(output) => {
                let label = format!("Survival analysis: {}", self.survival_time);
                self.apply_output(output, label);
                self.show_survival_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Survival analysis dialog.
    pub(super) fn survival_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_survival_dialog {
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
        // The catalogue borrow ends before the option is borrowed mutably.
        let none_label = self.catalog().label("None (all events)");
        let mut open = self.show_survival_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Survival Analysis"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(self.catalog().label("Time column:"));
                    egui::ComboBox::from_id_salt("survival_time")
                        .selected_text(if self.survival_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.survival_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.survival_time,
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
                    let label = ui.help_label(self.catalog().label("Censoring column:"));
                    egui::ComboBox::from_id_salt("survival_censor")
                        .selected_text(self.survival_censor.as_deref().unwrap_or(none_label))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.survival_censor, None, none_label);
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.survival_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(self.catalog().label("Censoring value:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.survival_censor_value)
                            .desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    let label = ui.help_label(self.catalog().label("Mission time:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.survival_mission).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(self.catalog().label(
                    "The censoring value marks right-censored observations; the mission time \
                     gives the reliability and the hazard of every fitted model.",
                ));
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About survival analysis…"))
                        .clicked()
                    {
                        self.show_survival_help = true;
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
        self.show_survival_dialog = open && !run && !cancel;
        if run {
            self.run_survival_dialog();
        }
    }

    /// Help page of the survival analysis.
    pub(super) fn survival_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_survival_help {
            return;
        }
        let mut open = self.show_survival_help;
        let mut close = false;
        dialog_window(self.catalog().label("Survival analysis help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The survival analysis fits the exponential, Weibull and lognormal life \
                     models to right-censored data and compares them by the maximized \
                     log-likelihood, the AIC and the BIC. It also gives the percentile lives \
                     B1, B5, B10 and B50 of every model and the reliability and hazard at a \
                     mission time, and draws the fitted curves over the Kaplan–Meier estimate.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose the numeric time column, an optional censoring column and the \
                     censoring value: an observation whose censoring column equals that value \
                     is treated as right-censored, every other one as a failure. Without a \
                     censoring column every observation is a failure. Times must be positive \
                     and at least one failure is needed. The mission time must be positive; \
                     the dialog starts it at the median of the times.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "Twelve times with the last three censored give an exponential mean of \
                     82/9; its B10 is about 0.96, its B50 about 6.32 and its reliability at \
                     5 is about 0.58. The AIC and the BIC order the three models, and the \
                     smallest value is the best compromise between fit and complexity.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_survival_help = open && !close;
    }

    pub fn open_weibull_dialog(&mut self) {
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
            self.set_error("Weibull reliability needs a numeric time column");
            return;
        }
        self.weibull_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.show_weibull_dialog = true;
    }
    pub fn run_weibull_dialog(&mut self) {
        if self.weibull_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.weibull_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::weibull_reliability(self.weibull_time.clone())
            .with_censoring(self.weibull_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Weibull reliability: {}", self.weibull_time));
        self.show_weibull_dialog = false;
    }
    pub(super) fn weibull_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_weibull_dialog {
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
        let mut open = self.show_weibull_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Weibull Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("weibull_time")
                        .selected_text(if self.weibull_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.weibull_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.weibull_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("weibull_censor")
                        .selected_text(
                            self.weibull_censor
                                .as_deref()
                                .unwrap_or("None (all events)"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.weibull_censor,
                                None,
                                "None (all events)",
                            );
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.weibull_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.weibull_censor_value)
                            .desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored Weibull MLE; shape optimized, scale profiled.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_weibull_dialog = open && !run;
        if run {
            self.run_weibull_dialog();
        }
    }
    pub fn open_exponential_dialog(&mut self) {
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
            self.set_error("Exponential reliability needs a numeric time column");
            return;
        }
        self.exp_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.show_exp_dialog = true;
    }
    pub fn run_exponential_dialog(&mut self) {
        if self.exp_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.exp_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::exponential_reliability(self.exp_time.clone())
            .with_censoring(self.exp_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Exponential reliability: {}", self.exp_time));
        self.show_exp_dialog = false;
    }
    pub(super) fn exponential_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_exp_dialog {
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
        let mut open = self.show_exp_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Exponential Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("exp_time")
                        .selected_text(if self.exp_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.exp_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.exp_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("exp_censor")
                        .selected_text(self.exp_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.exp_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.exp_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.exp_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored exponential MLE; θ = total time / events.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_exp_dialog = open && !run;
        if run {
            self.run_exponential_dialog();
        }
    }
    pub fn open_lognormal_dialog(&mut self) {
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
            self.set_error("Lognormal reliability needs a numeric time column");
            return;
        }
        self.log_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.show_log_dialog = true;
    }
    pub fn run_lognormal_dialog(&mut self) {
        if self.log_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.log_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::lognormal_reliability(self.log_time.clone())
            .with_censoring(self.log_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Lognormal reliability: {}", self.log_time));
        self.show_log_dialog = false;
    }
    pub(super) fn lognormal_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_log_dialog {
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
        let mut open = self.show_log_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Lognormal Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("log_time")
                        .selected_text(if self.log_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.log_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.log_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("log_censor")
                        .selected_text(self.log_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.log_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.log_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored lognormal MLE; coordinate ascent on the logarithm of x.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_log_dialog = open && !run;
        if run {
            self.run_lognormal_dialog();
        }
    }
    pub fn open_alt_dialog(&mut self) {
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
        self.alt_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.alt_stress = numeric
            .iter()
            .find(|name| **name != self.alt_time)
            .cloned()
            .unwrap_or_default();
        self.alt_mode.clear();
        self.alt_horizon.clear();
        self.show_alt_dialog = true;
    }
    pub fn run_alt_dialog(&mut self) {
        if self.alt_time.is_empty() || self.alt_stress.is_empty() {
            self.set_error("Select the time and the stress columns");
            return;
        }
        if self.alt_time == self.alt_stress {
            self.set_error("The time and the stress must be different columns");
            return;
        }
        let horizon = if self.alt_horizon.trim().is_empty() {
            None
        } else {
            match self.alt_horizon.trim().parse::<f64>() {
                Ok(value) if value > 0.0 => Some(value),
                _ => {
                    self.set_error("The horizon must be a positive number");
                    return;
                }
            }
        };
        let mode = (!self.alt_mode.is_empty()).then(|| self.alt_mode.clone());
        let spec = AnalysisSpec::accelerated_life(self.alt_time.clone(), self.alt_stress.clone())
            .with_accelerated_life_options(None, 1.0, self.alt_use_stress)
            .with_accelerated_life_model(self.alt_life, self.alt_acceleration)
            .with_competing_mode(mode)
            .with_competing_horizon(horizon);
        let label = format!("Accelerated Life: {}", self.alt_time);
        self.execute_spec(spec, label);
        self.show_alt_dialog = false;
    }
    pub(super) fn alt_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_alt_dialog {
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
        let all_names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_alt_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Accelerated Life Testing"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Time:", &mut self.alt_time),
                    ("Stress:", &mut self.alt_stress),
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
                    ui.add(egui::DragValue::new(&mut self.alt_use_stress).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Mode column:");
                    egui::ComboBox::from_id_salt("alt_mode")
                        .selected_text(if self.alt_mode.is_empty() {
                            "None".to_owned()
                        } else {
                            self.alt_mode.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.alt_mode, String::new(), "None");
                            for name in &all_names {
                                ui.selectable_value(
                                    &mut self.alt_mode,
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
                    let label = ui.help_label("Horizon:");
                    ui.text_edit_singleline(&mut self.alt_horizon)
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("empty = last failure");
                });
                ui.horizontal(|ui| {
                    ui.help_label("Life:");
                    for (life, label) in [
                        (fp_core::alt::LifeModel::Weibull, "Weibull"),
                        (fp_core::alt::LifeModel::Lognormal, "Lognormal"),
                        (fp_core::alt::LifeModel::Exponential, "Exponential"),
                    ] {
                        ui.selectable_value(&mut self.alt_life, life, label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.help_label("Acceleration:");
                    for (model, label) in [
                        (fp_core::alt::AccelerationModel::Arrhenius, "Arrhenius"),
                        (fp_core::alt::AccelerationModel::InversePower, "Power"),
                        (fp_core::alt::AccelerationModel::Eyring, "Eyring"),
                    ] {
                        ui.selectable_value(&mut self.alt_acceleration, model, label);
                    }
                });
                ui.small(
                    "Arrhenius/Weibull model: the stress should be an absolute temperature \
                     (kelvin); the life at the use stress is an extrapolation.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_alt_dialog = open && !run;
        if run {
            self.run_alt_dialog();
        }
    }
}
