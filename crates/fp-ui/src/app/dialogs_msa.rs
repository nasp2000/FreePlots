//! Measurement systems and multivariate tools: agreement, Gage R&R, Type 1,
//! linearity, Box-Cox, diagnostics, T2, k-means, PCA and the matrix plot.

use super::*;

impl FreePlotsApp {
    pub fn open_agreement_dialog(&mut self) {
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
        self.agreement_part = names.first().cloned().unwrap_or_default();
        self.agreement_appraiser = names.get(1).cloned().unwrap_or_default();
        self.agreement_rating = names.get(2).cloned().unwrap_or_default();
        self.show_agreement_dialog = true;
    }
    pub fn run_agreement_dialog(&mut self) {
        if self.agreement_part.is_empty()
            || self.agreement_appraiser.is_empty()
            || self.agreement_rating.is_empty()
        {
            self.set_error("Select the part, appraiser and rating columns");
            return;
        }
        if self.agreement_part == self.agreement_appraiser
            || self.agreement_part == self.agreement_rating
            || self.agreement_appraiser == self.agreement_rating
        {
            self.set_error("The three columns must be different");
            return;
        }
        let spec = AnalysisSpec::agreement(
            self.agreement_part.clone(),
            self.agreement_appraiser.clone(),
            self.agreement_rating.clone(),
        );
        self.execute_spec(spec, "Attribute Agreement".to_owned());
        self.show_agreement_dialog = false;
    }
    pub(super) fn agreement_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_agreement_dialog {
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
        let mut open = self.show_agreement_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Attribute Agreement"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Part:", &mut self.agreement_part),
                    ("Appraiser:", &mut self.agreement_appraiser),
                    ("Rating:", &mut self.agreement_rating),
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
                ui.small(
                    "Each appraiser should rate every part at least twice; kappa measures the \
                     agreement.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_agreement_dialog = open && !run;
        if run {
            self.run_agreement_dialog();
        }
    }
    pub fn open_box_cox_dialog(&mut self) {
        self.box_cox_column = self.selected.clone().unwrap_or_default();
        self.show_box_cox_dialog = true;
    }
    pub fn run_box_cox_dialog(&mut self) {
        if self.box_cox_column.is_empty() {
            self.set_error("Select the column to transform");
            return;
        }
        let spec = AnalysisSpec::box_cox(self.box_cox_column.clone());
        let label = format!("Box-Cox Transformation: {}", self.box_cox_column);
        self.execute_spec(spec, label);
        self.show_box_cox_dialog = false;
    }
    pub(super) fn box_cox_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_box_cox_dialog {
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
        let mut open = self.show_box_cox_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Box-Cox Transformation"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("box_cox_column")
                        .selected_text(if self.box_cox_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.box_cox_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.box_cox_column,
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
                    "The transformation needs positive values; the report compares the \
                     normality before and after.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_box_cox_dialog = open && !run;
        if run {
            self.run_box_cox_dialog();
        }
    }
    pub fn open_diagnostic_dialog(&mut self, kind: DiagnosticKind) {
        self.diagnostic_kind = kind;
        self.diagnostic_column = self.selected.clone().unwrap_or_default();
        self.diagnostic_expected = String::new();
        self.diagnostic_parameters = 0;
        self.diagnostic_alpha = 5.0;
        self.show_diagnostic_dialog = true;
    }
    pub fn run_diagnostic_dialog(&mut self) {
        if self.diagnostic_column.is_empty() {
            self.set_error("Select the numeric column");
            return;
        }
        let spec = match self.diagnostic_kind {
            DiagnosticKind::ChiSquareGoodnessOfFit => {
                let expected: Vec<f64> = self
                    .diagnostic_expected
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(|part| part.parse::<f64>().unwrap_or(f64::NAN))
                    .collect();
                if expected.len() < 2 {
                    self.set_error("Enter the expected counts as a comma-separated list");
                    return;
                }
                if expected.iter().any(|value| !value.is_finite()) {
                    self.set_error("The expected counts must be numbers");
                    return;
                }
                AnalysisSpec::chi_square_goodness_of_fit(self.diagnostic_column.clone(), expected)
                    .with_estimated_parameters(self.diagnostic_parameters)
            }
            DiagnosticKind::Grubbs => AnalysisSpec::grubbs(self.diagnostic_column.clone())
                .with_grubbs_alpha((self.diagnostic_alpha / 100.0).clamp(0.001, 0.5)),
        };
        let label = format!(
            "{}: {}",
            self.diagnostic_kind.name(),
            self.diagnostic_column
        );
        self.execute_spec(spec, label);
        self.show_diagnostic_dialog = false;
    }
    pub(super) fn diagnostic_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_diagnostic_dialog {
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
        let mut open = self.show_diagnostic_dialog;
        let mut run = false;
        dialog_window(self.diagnostic_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("diagnostic_column")
                        .selected_text(if self.diagnostic_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.diagnostic_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.diagnostic_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.diagnostic_kind == DiagnosticKind::ChiSquareGoodnessOfFit {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Expected counts:");
                        ui.text_edit_singleline(&mut self.diagnostic_expected)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Estimated parameters:");
                        ui.add(
                            egui::DragValue::new(&mut self.diagnostic_parameters)
                                .range(0..=10)
                                .speed(0.5),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.small(
                        "The column holds one observed count per cell; the expected counts are \
                         comma separated.",
                    );
                } else {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Significance:");
                        ui.add(
                            egui::DragValue::new(&mut self.diagnostic_alpha)
                                .range(0.1..=50.0)
                                .speed(0.5)
                                .suffix(" %"),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.small("The test looks for a single outlier in the column.");
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_diagnostic_dialog = open && !run;
        if run {
            self.run_diagnostic_dialog();
        }
    }
    pub fn open_gage_dialog(&mut self) {
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
            self.set_error("Gage R&R needs a numeric measurement column");
            return;
        }
        self.gage_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.gage_part = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.gage_measurement)
            .unwrap_or_default();
        self.gage_operator = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.gage_measurement && *name != self.gage_part)
            .unwrap_or_default();
        self.show_gage_dialog = true;
    }
    pub fn run_gage_dialog(&mut self) {
        if self.gage_measurement.is_empty()
            || self.gage_part.is_empty()
            || self.gage_operator.is_empty()
        {
            self.set_error("Select the measurement, part and operator columns");
            return;
        }
        if self.gage_measurement == self.gage_part
            || self.gage_measurement == self.gage_operator
            || self.gage_part == self.gage_operator
        {
            self.set_error("The three columns must be different");
            return;
        }
        let spec = AnalysisSpec::gage_rr(
            self.gage_measurement.clone(),
            self.gage_part.clone(),
            self.gage_operator.clone(),
        )
        .with_gage_design(self.gage_design);
        self.execute_spec(spec, format!("Gage R&R: {}", self.gage_measurement));
        self.show_gage_dialog = false;
    }
    pub(super) fn gage_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_gage_dialog {
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
        let mut open = self.show_gage_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage R&R"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("gage_measurement")
                        .selected_text(if self.gage_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.gage_measurement,
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
                    let label = ui.help_label("Design:");
                    egui::ComboBox::from_id_salt("gage_design")
                        .selected_text(self.gage_design.name())
                        .show_ui(ui, |ui| {
                            for design in [GageDesign::Crossed, GageDesign::Nested] {
                                ui.selectable_value(&mut self.gage_design, design, design.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Part:");
                    egui::ComboBox::from_id_salt("gage_part")
                        .selected_text(if self.gage_part.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_part.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.gage_part,
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
                    let label = ui.help_label("Operator:");
                    egui::ComboBox::from_id_salt("gage_operator")
                        .selected_text(if self.gage_operator.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_operator.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.gage_operator,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Balanced crossed design: same replicates in every part×operator cell.");
                ui.small("Crossed ANOVA method; interaction included.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_gage_dialog = open && !run;
        if run {
            self.run_gage_dialog();
        }
    }
    pub fn open_type1_dialog(&mut self) {
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
            self.set_error("Type 1 Gage needs a numeric measurement column");
            return;
        }
        self.type1_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.show_type1_dialog = true;
    }
    pub fn run_type1_dialog(&mut self) {
        if self.type1_measurement.is_empty() {
            self.set_error("Select the measurement column");
            return;
        }
        let Ok(reference) = self.type1_reference.trim().parse::<f64>() else {
            self.set_error("Reference value must be a number");
            return;
        };
        let parse_limit = |text: &str| -> Result<Option<f64>, String> {
            let text = text.trim();
            if text.is_empty() {
                Ok(None)
            } else {
                text.parse::<f64>()
                    .map(Some)
                    .map_err(|_| "Specification limits must be numbers".to_owned())
            }
        };
        let lsl = match parse_limit(&self.type1_lsl) {
            Ok(value) => value,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        let usl = match parse_limit(&self.type1_usl) {
            Ok(value) => value,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        let spec = AnalysisSpec::gage_type1(self.type1_measurement.clone(), reference)
            .with_gage_limits(lsl, usl);
        self.execute_spec(spec, format!("Type 1 Gage: {}", self.type1_measurement));
        self.show_type1_dialog = false;
    }
    pub(super) fn type1_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_type1_dialog {
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
        let mut open = self.show_type1_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage Type 1"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("type1_measurement")
                        .selected_text(if self.type1_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.type1_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.type1_measurement,
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
                    let label = ui.help_label("Reference:");
                    ui.text_edit_singleline(&mut self.type1_reference)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let lsl = ui.help_label("LSL:");
                    ui.text_edit_singleline(&mut self.type1_lsl)
                        .labelled_by(lsl.id)
                        .hover_from(&lsl);
                    let usl = ui.help_label("USL:");
                    ui.text_edit_singleline(&mut self.type1_usl)
                        .labelled_by(usl.id)
                        .hover_from(&usl);
                });
                ui.small("Repeated measurements of one reference part; limits optional.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_type1_dialog = open && !run;
        if run {
            self.run_type1_dialog();
        }
    }
    pub fn open_linearity_dialog(&mut self) {
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
        self.linearity_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.linearity_reference = numeric
            .iter()
            .find(|name| **name != self.linearity_measurement)
            .cloned()
            .unwrap_or_default();
        self.show_linearity_dialog = true;
    }
    pub fn run_linearity_dialog(&mut self) {
        if self.linearity_measurement.is_empty() || self.linearity_reference.is_empty() {
            self.set_error("Select the measurement and the reference columns");
            return;
        }
        if self.linearity_measurement == self.linearity_reference {
            self.set_error("The measurement and the reference must be different columns");
            return;
        }
        let spec = AnalysisSpec::gage_linearity(
            self.linearity_measurement.clone(),
            self.linearity_reference.clone(),
        );
        let label = format!("Gage Linearity: {}", self.linearity_measurement);
        self.execute_spec(spec, label);
        self.show_linearity_dialog = false;
    }
    pub(super) fn linearity_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_linearity_dialog {
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
        let mut open = self.show_linearity_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage Linearity and Bias"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("linearity_measurement")
                        .selected_text(if self.linearity_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.linearity_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.linearity_measurement,
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
                    let label = ui.help_label("Reference:");
                    egui::ComboBox::from_id_salt("linearity_reference")
                        .selected_text(if self.linearity_reference.is_empty() {
                            "—".to_owned()
                        } else {
                            self.linearity_reference.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.linearity_reference,
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
                    "Reference values must contain at least three distinct parts \
                     with repeated measurements.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_linearity_dialog = open && !run;
        if run {
            self.run_linearity_dialog();
        }
    }
    pub fn open_t2_dialog(&mut self) {
        self.t2_columns = self.selected.clone().unwrap_or_default();
        self.t2_subgroup = 3;
        self.t2_confidence = 95.0;
        self.show_t2_dialog = true;
    }
    pub fn run_t2_dialog(&mut self) {
        let columns: Vec<String> = self
            .t2_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::hotelling_t2(columns, self.t2_subgroup)
            .with_t2_confidence((self.t2_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, "Hotelling T² Chart".to_owned());
        self.show_t2_dialog = false;
    }
    pub(super) fn t2_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t2_dialog {
            return;
        }
        let mut open = self.show_t2_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Hotelling T² Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.t2_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let subgroup = ui.help_label("Subgroup:");
                    ui.add(egui::DragValue::new(&mut self.t2_subgroup).range(2..=25))
                        .labelled_by(subgroup.id)
                        .hover_from(&subgroup);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.t2_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                ui.small("Rows are taken in order; an incomplete last subgroup is dropped.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_t2_dialog = open && !run;
        if run {
            self.run_t2_dialog();
        }
    }
    pub fn open_kmeans_dialog(&mut self) {
        self.kmeans_columns = self.selected.clone().unwrap_or_default();
        self.kmeans_clusters = 3;
        self.kmeans_standardise = true;
        self.kmeans_seed = 2026;
        self.show_kmeans_dialog = true;
    }
    pub fn run_kmeans_dialog(&mut self) {
        let columns: Vec<String> = self
            .kmeans_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::kmeans(columns, self.kmeans_clusters)
            .with_kmeans_options(self.kmeans_standardise, self.kmeans_seed);
        self.execute_spec(spec, "K-Means Clustering".to_owned());
        self.show_kmeans_dialog = false;
    }
    pub(super) fn kmeans_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_kmeans_dialog {
            return;
        }
        let mut open = self.show_kmeans_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("K-Means Clustering"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.kmeans_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Clusters:");
                    ui.add(egui::DragValue::new(&mut self.kmeans_clusters).range(2..=20))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.help_checkbox(&mut self.kmeans_standardise, "Standardise (z scores)");
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.kmeans_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("The same seed gives the same clusters.");
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_kmeans_dialog = open && !run;
        if run {
            self.run_kmeans_dialog();
        }
    }
    pub fn open_pca_dialog(&mut self) {
        self.pca_columns = self.selected.clone().unwrap_or_default();
        self.pca_keep = 2;
        self.show_pca_dialog = true;
    }
    pub fn run_pca_dialog(&mut self) {
        let columns: Vec<String> = self
            .pca_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::pca(columns, self.pca_keep);
        self.execute_spec(spec, "Principal Component Analysis".to_owned());
        self.show_pca_dialog = false;
    }
    pub(super) fn pca_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pca_dialog {
            return;
        }
        let mut open = self.show_pca_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Principal Component Analysis"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.pca_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Score components:");
                    ui.add(egui::DragValue::new(&mut self.pca_keep).range(1..=10))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Comma-separated numeric columns; rows must be complete.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_pca_dialog = open && !run;
        if run {
            self.run_pca_dialog();
        }
    }
    pub fn open_matrix_dialog(&mut self) {
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
        self.matrix_columns = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| {
                numeric
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            });
        self.matrix_bins = 8;
        self.show_matrix_dialog = true;
    }
    pub fn run_matrix_dialog(&mut self) {
        let columns: Vec<String> = self
            .matrix_columns
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec =
            AnalysisSpec::matrix_plot(columns).with_matrix_bins(self.matrix_bins.clamp(3, 30));
        self.execute_spec(spec, "Matrix Plot".to_owned());
        self.show_matrix_dialog = false;
    }
    pub(super) fn matrix_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_matrix_dialog {
            return;
        }
        let mut open = self.show_matrix_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Matrix Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.matrix_columns)
                            .desired_width(250.0)
                            .hint_text("a,b,c"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Bins on the diagonal:");
                    ui.add(egui::DragValue::new(&mut self.matrix_bins).range(3..=30))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Up to six variables; rows must be complete.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_matrix_dialog = open && !run;
        if run {
            self.run_matrix_dialog();
        }
    }
}
