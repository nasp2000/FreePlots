// SPDX-License-Identifier: MIT
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
    /// Opens the Johnson transformation dialog with the selected column.
    pub fn open_johnson_dialog(&mut self) {
        self.johnson_column = self.selected.clone().unwrap_or_default();
        self.johnson_name = if self.johnson_column.is_empty() {
            "Johnson".to_owned()
        } else {
            format!("{}_J", self.johnson_column)
        };
        self.show_johnson_dialog = true;
    }

    /// Fits the Johnson transformation and writes the transformed column.
    pub fn run_johnson_dialog(&mut self) {
        if self.johnson_column.is_empty() {
            self.set_error("Select the column to transform");
            return;
        }
        if self.johnson_name.trim().is_empty() {
            self.set_error("Name the transformed column");
            return;
        }
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::johnson::add_transformation(
            &mut df,
            &self.johnson_column,
            self.johnson_choice,
            &self.johnson_name,
        ) {
            Ok(result) => {
                let name = self.johnson_name.trim().to_owned();
                self.set_data(df, "Johnson transformation".to_owned());
                self.append_session(&format!("{}\n", result.to_session_text()));
                self.status = format!("Column {name} added");
                self.show_johnson_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Johnson transformation dialog: family, output name and help.
    pub(super) fn johnson_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_johnson_dialog {
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
        let mut open = self.show_johnson_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Johnson Transformation"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("johnson_column")
                        .selected_text(if self.johnson_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.johnson_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.johnson_column,
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
                    let label = ui.help_label("Family:");
                    egui::ComboBox::from_id_salt("johnson_choice")
                        .selected_text(self.johnson_choice.name())
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for choice in [
                                fp_core::johnson::JohnsonChoice::Best,
                                fp_core::johnson::JohnsonChoice::Su,
                                fp_core::johnson::JohnsonChoice::Sb,
                                fp_core::johnson::JohnsonChoice::Sl,
                            ] {
                                ui.selectable_value(
                                    &mut self.johnson_choice,
                                    choice,
                                    choice.name(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.add(egui::TextEdit::singleline(&mut self.johnson_name).desired_width(160.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "Fits Johnson SU, SB or SL and writes the standard-normal scores as a new \
                     column; missing rows stay missing.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About the Johnson transformation…"))
                        .clicked()
                    {
                        self.show_johnson_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.johnson_column.is_empty(),
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
        self.show_johnson_dialog = open && !run && !cancel;
        if run {
            self.run_johnson_dialog();
        }
    }

    /// Help page of the Johnson transformation.
    pub(super) fn johnson_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_johnson_help {
            return;
        }
        let mut open = self.show_johnson_help;
        let mut close = false;
        dialog_window(self.catalog().label("Johnson Transformation help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The Johnson transformation maps a non-normal measurement to a \
                     standard-normal score. Three families cover different shapes: SL for data \
                     bounded below, SU for unbounded data, and SB for data between two limits. \
                     Choose Best (AIC) to fit all three and keep the best.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column with at least five values. Missing values are excluded \
                     from the fit and stay missing in the new column. The new column holds the \
                     score z and must have a name the worksheet does not use yet. The report \
                     shows the fitted parameters and the Anderson–Darling test before and after.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "Six measurements that rise steeply give a skewed histogram. Choose Johnson \
                     Transformation with Best (AIC): the command fits the best family, adds the \
                     column weight_J and reports the A² before and after. The new column is ready \
                     for the capability analysis or the control charts.",
                ));
                egui::Grid::new("johnson_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("observation");
                        ui.strong("weight");
                        ui.end_row();
                        for (index, weight) in [
                            ("1", "10.1"),
                            ("2", "10.3"),
                            ("3", "10.2"),
                            ("4", "10.6"),
                            ("5", "10.4"),
                            ("6", "11.0"),
                        ] {
                            ui.monospace(index);
                            ui.monospace(weight);
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
        self.show_johnson_help = open && !close;
    }

    /// Opens the individual distribution identification dialog.
    pub fn open_id_dialog(&mut self) {
        let column = self.selected.clone().unwrap_or_default();
        self.id_column = column.clone();
        if !column.is_empty() {
            if self.id_boxcox_name.trim().is_empty() {
                self.id_boxcox_name = format!("{column}_boxcox");
            }
            if self.id_johnson_name.trim().is_empty() {
                self.id_johnson_name = format!("{column}_johnson");
            }
        }
        self.show_id_dialog = true;
    }

    /// Runs the identification and stores the requested transformed columns.
    pub fn run_id_dialog(&mut self) {
        if self.id_column.is_empty() {
            self.set_error("Select the measurement column");
            return;
        }
        let column = self.id_column.clone();
        let box_cox = self
            .id_store_boxcox
            .then(|| self.id_boxcox_name.trim().to_owned());
        if self.id_store_boxcox && box_cox.as_deref().is_none_or(str::is_empty) {
            self.set_error("Name the Box-Cox column");
            return;
        }
        let johnson = self
            .id_store_johnson
            .then(|| self.id_johnson_name.trim().to_owned());
        if self.id_store_johnson && johnson.as_deref().is_none_or(str::is_empty) {
            self.set_error("Name the Johnson column");
            return;
        }
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::workflow::individual_distribution(
            &mut df,
            &column,
            box_cox.as_deref(),
            johnson.as_deref(),
        ) {
            Ok(output) => {
                self.set_data(df, "Individual distribution identification".to_owned());
                self.apply_output(output, format!("Distribution identification: {column}"));
                self.show_id_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Individual distribution identification dialog.
    pub(super) fn id_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_id_dialog {
            return;
        }
        let columns: Vec<String> = self
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
        let mut open = self.show_id_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(
            self.catalog()
                .label("Individual Distribution Identification"),
        )
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(470.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let label = ui.help_label("Column:");
                egui::ComboBox::from_id_salt("id_column")
                    .selected_text(if self.id_column.is_empty() {
                        "—".to_owned()
                    } else {
                        self.id_column.clone()
                    })
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for name in &columns {
                            ui.selectable_value(&mut self.id_column, name.clone(), name.as_str());
                        }
                    })
                    .response
                    .labelled_by(label.id)
                    .hover_from(&label);
            });
            ui.help_checkbox(&mut self.id_store_boxcox, "Store Box-Cox column");
            ui.horizontal(|ui| {
                let label = ui.help_label("Box-Cox name:");
                ui.add_enabled(
                    self.id_store_boxcox,
                    egui::TextEdit::singleline(&mut self.id_boxcox_name),
                )
                .labelled_by(label.id)
                .hover_from(&label);
            });
            ui.help_checkbox(&mut self.id_store_johnson, "Store Johnson column");
            ui.horizontal(|ui| {
                let label = ui.help_label("Johnson name:");
                ui.add_enabled(
                    self.id_store_johnson,
                    egui::TextEdit::singleline(&mut self.id_johnson_name),
                )
                .labelled_by(label.id)
                .hover_from(&label);
            });
            ui.small(
                "Fourteen distributions with the Anderson–Darling test and one probability \
                     panel per family; Box-Cox and Johnson store the transformed column.",
            );
            ui.horizontal(|ui| {
                if ui
                    .help_button(
                        self.catalog()
                            .label("About individual distribution identification…"),
                    )
                    .clicked()
                {
                    self.show_id_help = true;
                }
                ui.small(self.catalog().label("Overview, data and an example"));
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.id_column.is_empty(),
                        egui::Button::new(self.catalog().label("Run")),
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
        self.show_id_dialog = open && !run && !cancel;
        if run {
            self.run_id_dialog();
        }
    }

    /// Help page of the individual distribution identification.
    pub(super) fn id_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_id_help {
            return;
        }
        let mut open = self.show_id_help;
        let mut close = false;
        dialog_window(
            self.catalog()
                .label("Individual distribution identification help"),
        )
        .without_clear()
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.strong(self.catalog().label("Overview"));
            ui.label(self.catalog().label(
                "Individual distribution identification compares fourteen distributions on \
                     the same sample, with the Anderson–Darling test, AIC/BIC and one probability \
                     plot per family. Box-Cox and Johnson transformations can be stored as new \
                     worksheet columns for later analyses.",
            ));
            ui.add_space(6.0);
            ui.strong(self.catalog().label("Data"));
            ui.label(self.catalog().label(
                "Choose one numeric column; missing values are excluded and counted. Normal, \
                     logistic, extreme value and smallest extreme value accept any data; the other \
                     families need positive values. The Box-Cox and Johnson columns are written \
                     with the names in the dialog, and rows with a missing value stay missing.",
            ));
            ui.add_space(6.0);
            ui.strong(self.catalog().label("Example"));
            ui.label(self.catalog().label(
                "A right-skewed life column is fitted by the Weibull and the lognormal; the \
                     table ranks them by AIC and the panels show the probability plot of each \
                     family. Turning on Store Box-Cox column writes life_boxcox and turning on \
                     Store Johnson column writes life_johnson, both with the fitted score.",
            ));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.help_button(self.catalog().label("Close")).clicked() {
                    close = true;
                }
            });
        });
        self.show_id_help = open && !close;
    }

    pub fn open_diagnostic_dialog(&mut self, kind: DiagnosticKind) {
        self.diagnostic_kind = kind;
        self.diagnostic_column = self.selected.clone().unwrap_or_default();
        self.diagnostic_expected = String::new();
        self.diagnostic_parameters = 0;
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
        self.open_multivariate_dialog(fp_core::mvchart::MultivariateChart::HotellingT2);
    }

    /// Opens the multivariate chart dialog pre-set to `kind`.
    pub fn open_multivariate_dialog(&mut self, kind: fp_core::mvchart::MultivariateChart) {
        self.t2_kind = kind;
        self.t2_columns = self.selected.clone().unwrap_or_default();
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
        let confidence = (self.t2_confidence / 100.0).clamp(0.5, 0.999);
        if self.t2_kind == fp_core::mvchart::MultivariateChart::HotellingT2 {
            let spec = AnalysisSpec::hotelling_t2(columns, self.t2_subgroup)
                .with_t2_confidence(confidence);
            self.execute_spec(spec, "Hotelling T² Chart".to_owned());
            self.show_t2_dialog = false;
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let kind = self.t2_kind;
        let outcome = match kind {
            fp_core::mvchart::MultivariateChart::GeneralizedVariance => {
                fp_core::mvchart::generalized_variance(&df, &columns, self.t2_subgroup)
            }
            fp_core::mvchart::MultivariateChart::Mewma => {
                fp_core::mvchart::mewma(&df, &columns, self.t2_subgroup, self.t2_lambda, confidence)
            }
            fp_core::mvchart::MultivariateChart::HotellingT2 => unreachable!("handled above"),
        };
        match outcome {
            Ok(output) => {
                self.apply_output(output, kind.name().to_owned());
                self.show_t2_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    pub(super) fn t2_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t2_dialog {
            return;
        }
        let mut open = self.show_t2_dialog;
        let mut run = false;
        dialog_window(self.catalog().label(self.t2_kind.name()))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Chart:");
                    egui::ComboBox::from_id_salt("t2_kind")
                        .selected_text(self.t2_kind.name())
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for kind in [
                                fp_core::mvchart::MultivariateChart::HotellingT2,
                                fp_core::mvchart::MultivariateChart::GeneralizedVariance,
                                fp_core::mvchart::MultivariateChart::Mewma,
                            ] {
                                ui.selectable_value(&mut self.t2_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
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
                    let two_sided =
                        self.t2_kind != fp_core::mvchart::MultivariateChart::GeneralizedVariance;
                    ui.add_enabled(
                        two_sided,
                        egui::DragValue::new(&mut self.t2_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                if self.t2_kind == fp_core::mvchart::MultivariateChart::Mewma {
                    ui.horizontal(|ui| {
                        let lambda = ui.help_label("λ:");
                        ui.add(
                            egui::DragValue::new(&mut self.t2_lambda)
                                .range(0.01..=1.0)
                                .speed(0.01),
                        )
                        .labelled_by(lambda.id)
                        .hover_from(&lambda);
                        ui.small("smoothing constant");
                    });
                }
                ui.small(match self.t2_kind {
                    fp_core::mvchart::MultivariateChart::HotellingT2 => {
                        "Rows are taken in order; an incomplete last subgroup is dropped."
                    }
                    fp_core::mvchart::MultivariateChart::GeneralizedVariance => {
                        "Rows are taken in order; the subgroup size must be larger than the number \
                         of variables."
                    }
                    fp_core::mvchart::MultivariateChart::Mewma => {
                        "Rows are taken in order; the limit uses the Phase I F form."
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About multivariate charts…"))
                        .clicked()
                    {
                        self.show_mv_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
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

    /// Help page of the three multivariate charts.
    pub(super) fn mv_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_mv_help {
            return;
        }
        let mut open = self.show_mv_help;
        let mut close = false;
        dialog_window(self.catalog().label("Multivariate charts help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The multivariate charts monitor two or more correlated variables together. \
                     Hotelling's T² watches the mean vector, the generalized variance chart \
                     watches the joint variability, and the multivariate EWMA is more sensitive \
                     to small shifts in the mean.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose two or more numeric columns, separated by commas, and the subgroup \
                     size; rows are taken in order and an incomplete last subgroup is dropped. \
                     The T² and the multivariate EWMA use the confidence level; the generalized \
                     variance chart uses the 3-sigma b1/b2 limits and needs a subgroup larger \
                     than the number of variables. The multivariate EWMA also uses λ.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "With two variables and subgroups of three, the T² chart flags a subgroup \
                     whose mean vector leaves the F limit; the generalized variance chart flags \
                     a subgroup whose covariance determinant leaves the b1/b2 limits; λ = 0.2 \
                     smooths the mean vector for the EWMA chart.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_mv_help = open && !close;
    }
    pub fn open_kmeans_dialog(&mut self) {
        self.kmeans_columns = self.selected.clone().unwrap_or_default();
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
