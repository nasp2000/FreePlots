// SPDX-License-Identifier: MIT
//! Quality tools and multivariate dialogs: capability, control charts,
//! attribute charts, G chart, MSA, tolerance, sampling and the multivariate
//! methods.

use super::*;

impl FreePlotsApp {
    pub(super) fn capability_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_cap_dialog {
            return;
        }
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
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();

        let mut open = self.show_cap_dialog;
        let mut close = false;
        let mut pending: Option<AnalysisSpec> = None;
        let title = if self.cap_sixpack {
            self.catalog().label("Capability Sixpack")
        } else {
            self.catalog().label("Capability Analysis")
        };
        dialog_window(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("cap_column")
                        .selected_text(if self.cap_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.cap_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.cap_column,
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
                    let label = ui.help_label("Distribution:");
                    egui::ComboBox::from_id_salt("cap_distribution")
                        .selected_text(self.cap_distribution.name())
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for distribution in [
                                DistributionKind::Normal,
                                DistributionKind::Lognormal,
                                DistributionKind::Weibull,
                                DistributionKind::Exponential,
                                DistributionKind::Logistic,
                                DistributionKind::Loglogistic,
                                DistributionKind::Gamma,
                                DistributionKind::ExtremeValue,
                                DistributionKind::Weibull3,
                                DistributionKind::Lognormal3,
                            ] {
                                ui.selectable_value(
                                    &mut self.cap_distribution,
                                    distribution,
                                    distribution.name(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let lsl = ui.help_label("LSL:");
                    ui.text_edit_singleline(&mut self.cap_lsl)
                        .labelled_by(lsl.id)
                        .hover_from(&lsl);
                    let usl = ui.help_label("USL:");
                    ui.text_edit_singleline(&mut self.cap_usl)
                        .labelled_by(usl.id)
                        .hover_from(&usl);
                });
                ui.horizontal(|ui| {
                    let target = ui.help_label("Target:");
                    ui.text_edit_singleline(&mut self.cap_target)
                        .labelled_by(target.id)
                        .hover_from(&target);
                    ui.small("empty = midpoint for Cpm");
                });
                ui.horizontal(|ui| {
                    let subgroup = ui.help_label("Subgroup size:");
                    ui.add_enabled(
                        self.cap_subgroup_column.is_empty(),
                        egui::DragValue::new(&mut self.cap_subgroup).range(1..=100),
                    )
                    .labelled_by(subgroup.id)
                    .hover_from(&subgroup);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.cap_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                if self.cap_distribution != DistributionKind::Normal {
                    self.cap_subgroup_column.clear();
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Subgroup column:");
                    let enabled = self.cap_distribution == DistributionKind::Normal;
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt("cap_subgroup_column")
                            .selected_text(if self.cap_subgroup_column.is_empty() {
                                "None".to_owned()
                            } else {
                                self.cap_subgroup_column.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.cap_subgroup_column,
                                    String::new(),
                                    "None",
                                );
                                for name in &all_columns {
                                    ui.selectable_value(
                                        &mut self.cap_subgroup_column,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.small("groups the rows by label");
                });
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        self.cap_distribution == DistributionKind::Normal && !self.cap_sixpack,
                        egui::Checkbox::new(&mut self.cap_box_cox, "Box-Cox transformation"),
                    )
                    .on_hover_text(checkbox_help("Box-Cox transformation"));
                    ui.small("positive data; Normal model only");
                });
                ui.help_checkbox(&mut self.cap_sixpack, "Sixpack (six panels)");
                if self.cap_sixpack {
                    self.cap_box_cox = false;
                    ui.small(
                        "The Sixpack is a normal-model view: histogram, probability plot, \
                         control chart, last 25 and capability indices. Box-Cox and other \
                         distributions are not available.",
                    );
                }
                ui.help_checkbox(
                    &mut self.cap_require_stability,
                    "Require a stable I–MR screen",
                );
                ui.horizontal(|ui| {
                    let label = ui.help_label("Bootstrap reps:");
                    ui.add(
                        egui::DragValue::new(&mut self.cap_bootstrap)
                            .range(0..=2000)
                            .speed(10),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    ui.small("0 = auto for non-normal PPM");
                });
                if self.cap_distribution != DistributionKind::Normal {
                    ui.small(
                        "Non-normal capability fits by MLE and reports expected/observed PPM; \
                         subgroup, confidence and Cpm target apply only to the normal model.",
                    );
                }
                ui.small("At least one specification limit is required.");
                ui.horizontal(|ui| {
                    if self.cap_sixpack {
                        if ui
                            .help_button(self.catalog().label("About the six panels…"))
                            .clicked()
                        {
                            self.show_sixpack_help = true;
                        }
                    } else if ui
                        .help_button(self.catalog().label("How to choose subgroups…"))
                        .clicked()
                    {
                        self.show_cap_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.cap_column.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        let parse = |text: &str| -> Result<Option<f64>, String> {
                            let text = text.trim();
                            if text.is_empty() {
                                return Ok(None);
                            }
                            text.parse::<f64>()
                                .map(Some)
                                .map_err(|_| format!("invalid limit: {text}"))
                        };
                        match (
                            parse(&self.cap_lsl),
                            parse(&self.cap_usl),
                            parse(&self.cap_target),
                        ) {
                            (Ok(lsl), Ok(usl), Ok(target)) => {
                                if lsl.is_none() && usl.is_none() {
                                    self.set_error(
                                        "Capability needs at least one specification limit",
                                    );
                                } else {
                                    let mut spec =
                                        AnalysisSpec::capability(self.cap_column.clone(), lsl, usl)
                                            .with_capability_distribution(self.cap_distribution)
                                            .with_capability_target(target)
                                            .with_capability_bootstrap(self.cap_bootstrap)
                                            .with_capability_box_cox(
                                                self.cap_box_cox
                                                    && !self.cap_sixpack
                                                    && self.cap_distribution
                                                        == DistributionKind::Normal,
                                            )
                                            .with_capability_stability(self.cap_require_stability)
                                            .with_capability_sixpack(self.cap_sixpack)
                                            .with_capability_subgroup_column(
                                                (self.cap_distribution == DistributionKind::Normal
                                                    && !self.cap_subgroup_column.is_empty())
                                                .then(|| self.cap_subgroup_column.clone()),
                                            );
                                    if let AnalysisSpec::Capability {
                                        subgroup: size,
                                        confidence,
                                        ..
                                    } = &mut spec
                                    {
                                        *size = if self.cap_subgroup_column.is_empty() {
                                            self.cap_subgroup
                                        } else {
                                            1
                                        };
                                        *confidence = self.cap_confidence / 100.0;
                                    }
                                    pending = Some(spec);
                                    close = true;
                                }
                            }
                            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                                self.set_error(error)
                            }
                        }
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_cap_dialog = open && !close;
        if let Some(spec) = pending {
            let label = format!("Capability: {}", spec.column());
            self.execute_spec(spec, label);
        }
    }
    /// Help page of the capability dialog.
    ///
    /// The same three parts as the chart help: an overview, the data shapes the
    /// analysis accepts and a worked example with a label column.
    pub(super) fn capability_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_cap_help {
            return;
        }
        let mut open = self.show_cap_help;
        let mut close = false;
        dialog_window(self.catalog().label("Capability help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "Capability compares the spread of the process with the specification limits. \
                     The within estimate uses the variation inside the subgroups, so the indices \
                     describe what the process could do once the shifts between subgroups are \
                     removed.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the measurements. Either set a constant subgroup \
                     size, which takes consecutive rows, or choose a column whose labels say \
                     which subgroup each row belongs to. Rows with a missing measurement or \
                     label are not used. Each subgroup needs at least two observations and the \
                     analysis needs at least two subgroups. The subgroup column applies to the \
                     normal model.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A line measures three parts twice. Put the measurements in weight and the \
                     part name in part, set the specification limits, and choose part as the \
                     subgroup column. Cp and Cpk come from the pooled within-subgroup standard \
                     deviation (n − g degrees of freedom); Pp and Ppk come from the overall \
                     standard deviation.",
                ));
                egui::Grid::new("capability_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("weight");
                        ui.strong("part");
                        ui.end_row();
                        for (weight, part) in [
                            ("10.02", "A"),
                            ("10.05", "A"),
                            ("9.98", "B"),
                            ("10.01", "B"),
                            ("10.04", "C"),
                            ("10.00", "C"),
                        ] {
                            ui.monospace(weight);
                            ui.monospace(part);
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
        self.show_cap_help = open && !close;
    }

    /// Help page of the capability Sixpack.
    ///
    /// The overview, the data shapes and a worked example of the six panels.
    pub(super) fn sixpack_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_sixpack_help {
            return;
        }
        let mut open = self.show_sixpack_help;
        let mut close = false;
        dialog_window(self.catalog().label("Capability Sixpack help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The Sixpack shows one normal capability study as six panels. The control \
                     chart and the histogram describe the process, the probability plot checks \
                     the normal model, and the capability panel compares the spread with the \
                     specification limits.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the measurements. Set a constant subgroup size, or \
                     choose a column whose labels say which subgroup each row belongs to. The \
                     control chart uses individuals when the subgroup size is 1 and the subgroup \
                     means otherwise. The Sixpack uses the normal model and does not apply \
                     Box-Cox.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A line measures three parts twice. Put the measurements in weight and the \
                     part name in part, choose Capability Sixpack, set the specification limits \
                     and part as the subgroup column. The panels show the histogram, the normal \
                     probability plot, the X̄ and R charts, the last 25 points and the capability \
                     indices.",
                ));
                egui::Grid::new("sixpack_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("weight");
                        ui.strong("part");
                        ui.end_row();
                        for (weight, part) in [
                            ("10.02", "A"),
                            ("10.05", "A"),
                            ("9.98", "B"),
                            ("10.01", "B"),
                            ("10.04", "C"),
                            ("10.00", "C"),
                        ] {
                            ui.monospace(weight);
                            ui.monospace(part);
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
        self.show_sixpack_help = open && !close;
    }

    /// Opens the control-chart dialog pre-set to the CUSUM chart.
    pub fn open_cusum_dialog(&mut self) {
        if let Some(column) = &self.selected {
            self.chart_column = column.clone();
        }
        self.chart_kind = ControlChartKind::Cusum;
        // Phases and a subgroup column belong to the variable charts; a
        // leftover choice from a previous chart would not be used here.
        self.chart_phase_column.clear();
        self.chart_subgroup_column.clear();
        self.show_chart_dialog = true;
    }

    /// Opens the acceptance chart dialog with the selected column.
    pub fn open_acceptance_dialog(&mut self) {
        if let Some(column) = &self.selected {
            self.acceptance_column = column.clone();
        }
        self.show_acceptance_dialog = true;
    }

    /// Builds and runs the acceptance chart of the dialog.
    pub fn run_acceptance_dialog(&mut self) {
        if self.acceptance_column.is_empty() {
            self.set_error("Select the measurement column");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        fn parse_limit(text: &str, name: &str) -> Result<Option<f64>, String> {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            match trimmed.parse::<f64>() {
                Ok(value) if value.is_finite() => Ok(Some(value)),
                _ => Err(format!("{name} must be a finite number")),
            }
        }
        let lower_spec = match parse_limit(&self.acceptance_lsl, "The lower specification") {
            Ok(value) => value,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let upper_spec = match parse_limit(&self.acceptance_usl, "The upper specification") {
            Ok(value) => value,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let sigma = match parse_limit(&self.acceptance_sigma, "The known sigma") {
            Ok(None) => None,
            Ok(Some(value)) if value > 0.0 => Some(value),
            Ok(Some(_)) => {
                self.set_error("The known sigma must be positive");
                return;
            }
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        let options = fp_core::acceptance::AcceptanceOptions {
            lower_spec,
            upper_spec,
            subgroup: self.acceptance_subgroup,
            sigma,
            acceptable: self.acceptance_p1,
            rejectable: self.acceptance_p2,
            producer: self.acceptance_alpha,
            consumer: self.acceptance_beta,
        };
        match fp_core::acceptance::analyze(&df, &self.acceptance_column, &options) {
            Ok(output) => {
                let label = format!("Acceptance chart: {}", self.acceptance_column);
                self.apply_output(output, label);
                self.show_acceptance_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Acceptance chart dialog.
    pub(super) fn acceptance_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_acceptance_dialog {
            return;
        }
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
        let mut open = self.show_acceptance_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Acceptance Chart"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("acceptance_column")
                        .selected_text(if self.acceptance_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.acceptance_column.clone()
                        })
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.acceptance_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                for (label_text, field, hint) in [
                    (
                        "Lower spec (LSL):",
                        &mut self.acceptance_lsl,
                        "blank = none",
                    ),
                    (
                        "Upper spec (USL):",
                        &mut self.acceptance_usl,
                        "blank = none",
                    ),
                    (
                        "Known sigma:",
                        &mut self.acceptance_sigma,
                        "blank = estimate",
                    ),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(label_text);
                        ui.text_edit_singleline(field)
                            .labelled_by(label.id)
                            .hover_from(&label);
                        ui.small(hint);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Subgroup size:");
                    ui.add(
                        egui::DragValue::new(&mut self.acceptance_subgroup)
                            .range(1..=25)
                            .speed(0.1),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    ui.small("1 = individuals");
                });
                for (label_text, field, low) in [
                    ("Accepted p1:", &mut self.acceptance_p1, 0.000_01),
                    ("Rejectable p2:", &mut self.acceptance_p2, 0.000_1),
                    ("Producer risk α:", &mut self.acceptance_alpha, 0.000_1),
                    ("Consumer risk β:", &mut self.acceptance_beta, 0.000_1),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(label_text);
                        ui.add(egui::DragValue::new(field).range(low..=0.49).speed(0.001))
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small(
                    "Acceptance and rejection limits come from the specification: inside the \
                     acceptance limits accept, beyond a rejection limit reject, between them \
                     continue sampling.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About acceptance charts…"))
                        .clicked()
                    {
                        self.show_acceptance_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.acceptance_column.is_empty(),
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
        self.show_acceptance_dialog = open && !run && !cancel;
        if run {
            self.run_acceptance_dialog();
        }
    }

    /// Help page of the acceptance chart.
    pub(super) fn acceptance_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_acceptance_help {
            return;
        }
        let mut open = self.show_acceptance_help;
        let mut close = false;
        dialog_window(self.catalog().label("Acceptance chart help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The acceptance chart plots the mean of each subgroup against limits \
                     derived from the specification. It accepts a process whose mean is inside \
                     the acceptance limits, rejects a process whose mean is beyond a rejection \
                     limit, and asks for another subgroup in between.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the measurements and at least one specification \
                     limit is required. The accepted proportion p1 and the rejectable proportion \
                     p2 are the proportions allowed outside a specification limit; the producer \
                     risk α and the consumer risk β set how often each decision may be wrong. \
                     Leave sigma blank to estimate it from the average subgroup range, or enter \
                     a known value. A subgroup size of 1 charts the individuals.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "With LSL 0 and USL 10, sigma 1, p1 0.001, p2 0.05, α 0.05 and β 0.10, the \
                     upper acceptance limit is 8.555 and the upper rejection limit is 9.637. A \
                     subgroup mean of 9.0 continues, 9.8 rejects and 5.0 accepts.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_acceptance_help = open && !close;
    }

    pub(super) fn control_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_chart_dialog {
            return;
        }
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
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_chart_dialog;
        let mut close = false;
        let mut pending: Option<(
            String,
            ControlChartKind,
            usize,
            TestSet,
            fp_core::spc::CusumOptions,
            fp_core::spc::EwmaOptions,
        )> = None;
        dialog_window(self.catalog().label("Control Chart"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("chart_column")
                        .selected_text(if self.chart_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.chart_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.chart_column,
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
                    let label = ui.help_label("Chart:");
                    egui::ComboBox::from_id_salt("chart_kind")
                        .selected_text(self.chart_kind.name())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for kind in [
                                ControlChartKind::IndividualsMovingRange,
                                ControlChartKind::XbarR,
                                ControlChartKind::XbarS,
                                ControlChartKind::Ewma,
                                ControlChartKind::Cusum,
                            ] {
                                if ui
                                    .selectable_value(&mut self.chart_kind, kind, kind.name())
                                    .clicked()
                                    && !matches!(
                                        kind,
                                        ControlChartKind::IndividualsMovingRange
                                            | ControlChartKind::XbarR
                                            | ControlChartKind::XbarS
                                    )
                                {
                                    self.chart_phase_column.clear();
                                }
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Subgroup column:");
                    let enabled = matches!(
                        self.chart_kind,
                        ControlChartKind::XbarR | ControlChartKind::XbarS
                    ) && self.chart_phase_column.is_empty();
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt("chart_subgroup_column")
                            .selected_text(if self.chart_subgroup_column.is_empty() {
                                "None".to_owned()
                            } else {
                                self.chart_subgroup_column.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.chart_subgroup_column,
                                    String::new(),
                                    "None",
                                );
                                for name in &all_columns {
                                    ui.selectable_value(
                                        &mut self.chart_subgroup_column,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Phase column (optional):");
                    let enabled = matches!(
                        self.chart_kind,
                        ControlChartKind::IndividualsMovingRange
                            | ControlChartKind::XbarR
                            | ControlChartKind::XbarS
                    );
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt("chart_phase_column")
                            .selected_text(if self.chart_phase_column.is_empty() {
                                "None".to_owned()
                            } else {
                                self.chart_phase_column.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.chart_phase_column,
                                    String::new(),
                                    "None",
                                );
                                for name in &all_columns {
                                    ui.selectable_value(
                                        &mut self.chart_phase_column,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                });
                if !self.chart_phase_column.is_empty() {
                    ui.small(
                        "Each consecutive run of the phase column gets its own limits; the \
                         subgroup column is not used.",
                    );
                }
                ui.horizontal(|ui| {
                    let cusum = self.chart_kind == ControlChartKind::Cusum;
                    let enabled = (matches!(
                        self.chart_kind,
                        ControlChartKind::XbarR | ControlChartKind::XbarS
                    ) && self.chart_subgroup_column.is_empty())
                        || cusum;
                    let mut value = egui::DragValue::new(&mut self.chart_subgroup);
                    value = if cusum {
                        value.range(1..=25)
                    } else {
                        value.range(2..=25)
                    };
                    let label = ui.help_label("Subgroup size:");
                    ui.add_enabled(enabled, value)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "The CUSUM chart uses the individuals when the subgroup size is 1 and the \
                     subgroup means otherwise.",
                );
                if self.chart_kind == ControlChartKind::Cusum {
                    ui.horizontal(|ui| {
                        let k = ui.help_label("k:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_cusum_k)
                                .range(0.05..=5.0)
                                .speed(0.05),
                        )
                        .labelled_by(k.id)
                        .hover_from(&k);
                        let h = ui.help_label("h:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_cusum_h)
                                .range(0.5..=20.0)
                                .speed(0.1),
                        )
                        .labelled_by(h.id)
                        .hover_from(&h);
                        ui.help_label("σ");
                    });
                }
                if self.chart_kind == ControlChartKind::Ewma {
                    ui.horizontal(|ui| {
                        let lambda = ui.help_label("λ:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_ewma_lambda)
                                .range(0.01..=1.0)
                                .speed(0.01),
                        )
                        .labelled_by(lambda.id)
                        .hover_from(&lambda);
                        let limit = ui.help_label("L:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_ewma_limit)
                                .range(0.5..=10.0)
                                .speed(0.1),
                        )
                        .labelled_by(limit.id)
                        .hover_from(&limit);
                        ui.help_label("σ");
                    });
                }
                ui.small("Control limits use the published Shewhart constants.");
                ui.horizontal(|ui| {
                    if self.chart_kind == ControlChartKind::Cusum {
                        if ui
                            .help_button(self.catalog().label("About CUSUM…"))
                            .clicked()
                        {
                            self.show_cusum_help = true;
                        }
                    } else if ui
                        .help_button(self.catalog().label("How to choose subgroups…"))
                        .clicked()
                    {
                        self.show_chart_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.help_label("Tests for special causes:");
                let descriptions = [
                    "1  point outside a control limit",
                    "2  K points in a row on the same side of the center line",
                    "3  K points in a row, all increasing or all decreasing",
                    "4  K points in a row, alternating up and down",
                    "5  K out of K+1 points more than 2σ from the center line, same side",
                    "6  K out of K+1 points more than 1σ from the center line, same side",
                    "7  K points in a row within 1σ of the center line",
                    "8  K points in a row more than 1σ from the center line",
                ];
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        for column in 0..4 {
                            let test = (row * 4 + column + 1) as u8;
                            let mut enabled = self.chart_tests.is_enabled(test);
                            if ui
                                .checkbox(&mut enabled, test.to_string())
                                .on_hover_text(descriptions[(test - 1) as usize])
                                .changed()
                                && let Ok(updated) = self.chart_tests.set_enabled(test, enabled)
                            {
                                self.chart_tests = updated;
                            }
                        }
                    });
                }
                egui::CollapsingHeader::new("Sensitivity (DEFTEST)")
                    .id_salt("chart_sensitivity")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for test in 1..=8u8 {
                                let label = ui.help_label(format!("K{test}"));
                                let mut value = i32::from(self.chart_tests.sensitivity(test));
                                let response = ui.add_enabled(
                                    test != 1 && self.chart_tests.is_enabled(test),
                                    egui::DragValue::new(&mut value).range(1..=15).speed(0.1),
                                );
                                if response.changed()
                                    && let Ok(updated) =
                                        self.chart_tests.with_sensitivity(test, value as u8)
                                {
                                    self.chart_tests = updated;
                                }
                                response.labelled_by(label.id).hover_from(&label);
                            }
                        });
                        ui.small(
                            "One K per test, in test order. Test 1 is fixed at three sigma \
                             because that is where the limits are drawn.",
                        );
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.chart_column.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        let subgroup = if matches!(
                            self.chart_kind,
                            ControlChartKind::IndividualsMovingRange | ControlChartKind::Ewma
                        ) {
                            1
                        } else {
                            self.chart_subgroup
                        };
                        let cusum =
                            fp_core::spc::CusumOptions::new(self.chart_cusum_k, self.chart_cusum_h)
                                .unwrap_or_default();
                        let ewma = fp_core::spc::EwmaOptions::new(
                            self.chart_ewma_lambda,
                            self.chart_ewma_limit,
                        )
                        .unwrap_or_default();
                        pending = Some((
                            self.chart_column.clone(),
                            self.chart_kind,
                            subgroup,
                            self.chart_tests,
                            cusum,
                            ewma,
                        ));
                        close = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_chart_dialog = open && !close;
        if let Some((column, kind, subgroup, tests, cusum, ewma)) = pending {
            self.run_control_chart_with_options(column, kind, subgroup, tests, cusum, ewma);
        }
    }
    /// Help page of the control-chart dialog.
    ///
    /// A short overview, the data shapes the chart accepts and a worked example
    /// with a label column. It sits beside the dialog so the explanation of the
    /// subgroup column is one click away from the field.
    pub(super) fn chart_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_chart_help {
            return;
        }
        let mut open = self.show_chart_help;
        let mut close = false;
        dialog_window(self.catalog().label("Control chart help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The X̄–R and X̄–S charts follow the mean and the spread of each subgroup. \
                     The limits come from the variation inside the subgroups, so a slow drift \
                     does not widen them.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the measurements. Either set a constant subgroup \
                     size, which takes consecutive rows, or choose a column whose labels say \
                     which subgroup each row belongs to. Labels may repeat in any order: the \
                     chart keeps the order in which each label first appears. Rows with a \
                     missing measurement or label are not used. Each subgroup needs 2 to 25 \
                     observations, and the chart needs at least two subgroups.",
                ));
                ui.label(self.catalog().label(
                    "The phase column and the subgroup column are alternatives: a phase gives \
                     each consecutive run its own limits, a subgroup column groups the rows \
                     before the limits are computed.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A line measures three parts twice. Put the measurements in weight and the \
                     part name in part, choose X̄–R as the chart and part as the subgroup \
                     column. The Session reports one point per part, the size of each subgroup \
                     and the limits computed from the pooled within-subgroup variation.",
                ));
                egui::Grid::new("chart_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("weight");
                        ui.strong("part");
                        ui.end_row();
                        for (weight, part) in [
                            ("10.02", "A"),
                            ("10.05", "A"),
                            ("9.98", "B"),
                            ("10.01", "B"),
                            ("10.04", "C"),
                            ("10.00", "C"),
                        ] {
                            ui.monospace(weight);
                            ui.monospace(part);
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
        self.show_chart_help = open && !close;
    }

    /// Help page of the CUSUM chart.
    ///
    /// The overview, the data shapes and a worked example of the two
    /// cumulative sums.
    pub(super) fn cusum_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_cusum_help {
            return;
        }
        let mut open = self.show_cusum_help;
        let mut close = false;
        dialog_window(self.catalog().label("CUSUM help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The CUSUM chart accumulates the standardised deviations from the centre \
                     line on two sides. A small shift that a Shewhart chart misses makes the \
                     sums drift, so the chart reacts to a sustained change of about one sigma.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the measurements. A subgroup size of 1 charts the \
                     individuals; a larger subgroup charts the subgroup means. The reference \
                     value k is half the shift to detect (0.5 sigma by default) and the decision \
                     interval h is the distance that raises a signal (4 sigma by default). Both \
                     are set in the dialog. Rows with a missing measurement are not used.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A process centred at 10 with sigma 1 shifts to 11. With k = 0.5 and h = 4 \
                     each point adds about 1 − 0.5 = 0.5 to the upper sum, which crosses the \
                     decision interval after eight points; the lower sum stays at zero.",
                ));
                egui::Grid::new("cusum_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("observation");
                        ui.strong("value");
                        ui.end_row();
                        for (index, value) in [
                            ("1", "10.1"),
                            ("2", "10.2"),
                            ("3", "10.4"),
                            ("4", "10.6"),
                            ("5", "10.8"),
                            ("6", "11.0"),
                            ("7", "11.0"),
                            ("8", "11.1"),
                        ] {
                            ui.monospace(index);
                            ui.monospace(value);
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
        self.show_cusum_help = open && !close;
    }

    pub(super) fn attribute_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_attr_dialog {
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
        let mut open = self.show_attr_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Attribute Control Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Chart:");
                    egui::ComboBox::from_id_salt("attr_kind")
                        .selected_text(self.attr_kind.name())
                        .show_ui(ui, |ui| {
                            for kind in [
                                AttributeChartKind::P,
                                AttributeChartKind::Np,
                                AttributeChartKind::C,
                                AttributeChartKind::U,
                                AttributeChartKind::LaneyP,
                                AttributeChartKind::LaneyNp,
                                AttributeChartKind::LaneyC,
                                AttributeChartKind::LaneyU,
                            ] {
                                ui.selectable_value(&mut self.attr_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Defects column:");
                    egui::ComboBox::from_id_salt("attr_column")
                        .selected_text(if self.attr_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.attr_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.attr_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.attr_kind.needs_size() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Size column:");
                        egui::ComboBox::from_id_salt("attr_sizes")
                            .selected_text(
                                self.attr_sizes.as_deref().unwrap_or("None (use constant)"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.attr_sizes,
                                    None,
                                    "None (use constant)",
                                );
                                for name in &columns {
                                    ui.selectable_value(
                                        &mut self.attr_sizes,
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
                        let label = ui.help_label("Constant size:");
                        ui.add_enabled(
                            self.attr_sizes.is_none(),
                            egui::TextEdit::singleline(&mut self.attr_size).desired_width(90.0),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                ui.small(match self.attr_kind {
                    AttributeChartKind::P => {
                        "Proportion defective; variable subgroup sizes allowed."
                    }
                    AttributeChartKind::Np => {
                        "Number defective; the subgroup size must be constant."
                    }
                    AttributeChartKind::C => {
                        "Defects per sample; constant opportunity (no denominator)."
                    }
                    AttributeChartKind::U => "Defects per unit; variable subgroup sizes allowed.",
                    AttributeChartKind::LaneyP => {
                        "Proportion defective with Laney limits adjusted for overdispersion."
                    }
                    AttributeChartKind::LaneyNp => {
                        "Number defective with Laney limits; the subgroup size must be constant."
                    }
                    AttributeChartKind::LaneyC => {
                        "Defects per sample with Laney limits; constant opportunity."
                    }
                    AttributeChartKind::LaneyU => {
                        "Defects per unit with Laney limits; variable subgroup sizes allowed."
                    }
                });
                ui.small("Tests 1 to 4 are applied to the plotted values.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_attr_dialog = open && !run;
        if run {
            self.run_attribute_chart();
        }
    }
    pub fn open_attribute_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if !df
            .columns()
            .iter()
            .any(|column| matches!(column.dtype(), DType::Float | DType::Integer))
        {
            self.set_error("No numeric column available");
            return;
        }
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_sizes = None;
        self.show_attr_dialog = true;
    }

    /// Opens the attribute capability dialog with the selected column.
    pub fn open_attrcap_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if !df
            .columns()
            .iter()
            .any(|column| matches!(column.dtype(), DType::Float | DType::Integer))
        {
            self.set_error("No numeric column available");
            return;
        }
        self.attrcap_defects = self.selected.clone().unwrap_or_default();
        self.attrcap_sizes = None;
        self.show_attrcap_dialog = true;
    }

    /// Attribute capability dialog: binomial (defectives) or Poisson (defects).
    pub(super) fn attrcap_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_attrcap_dialog {
            return;
        }
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

        let mut open = self.show_attrcap_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Attribute Capability"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Distribution:");
                    egui::ComboBox::from_id_salt("attrcap_kind")
                        .selected_text(self.attrcap_kind.name())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for kind in [
                                fp_core::attrcap::AttributeCapabilityKind::Binomial,
                                fp_core::attrcap::AttributeCapabilityKind::Poisson,
                            ] {
                                ui.selectable_value(&mut self.attrcap_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Defects column:");
                    egui::ComboBox::from_id_salt("attrcap_defects")
                        .selected_text(if self.attrcap_defects.is_empty() {
                            "—".to_owned()
                        } else {
                            self.attrcap_defects.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.attrcap_defects,
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
                    let label = ui.help_label("Size column:");
                    egui::ComboBox::from_id_salt("attrcap_sizes")
                        .selected_text(
                            self.attrcap_sizes
                                .as_deref()
                                .unwrap_or("None (use constant)"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.attrcap_sizes,
                                None,
                                "None (use constant)",
                            );
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.attrcap_sizes,
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
                    let label = ui.help_label("Constant size:");
                    ui.add_enabled(
                        self.attrcap_sizes.is_none(),
                        egui::TextEdit::singleline(&mut self.attrcap_size).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.attrcap_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                ui.small(match self.attrcap_kind {
                    fp_core::attrcap::AttributeCapabilityKind::Binomial => {
                        "Binomial: defectives per subgroup and the inspected units; reports PPM \
                         defective."
                    }
                    fp_core::attrcap::AttributeCapabilityKind::Poisson => {
                        "Poisson: defects per subgroup and the units; reports PPM defects and DPU."
                    }
                });
                ui.small("The exact interval and process Z are reported with the PPM.");
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About attribute capability…"))
                        .clicked()
                    {
                        self.show_attrcap_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.attrcap_defects.is_empty(),
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
        self.show_attrcap_dialog = open && !run && !cancel;
        if run {
            self.run_attrcap_dialog();
        }
    }

    /// Help page of the attribute capability dialog.
    pub(super) fn attrcap_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_attrcap_help {
            return;
        }
        let mut open = self.show_attrcap_help;
        let mut close = false;
        dialog_window(self.catalog().label("Attribute Capability help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "Attribute capability turns counted defectives or defects into parts per \
                     million. The binomial study pools the defectives over the inspected units \
                     and reports the proportion defective, its PPM and the normal-equivalent \
                     process Z. The Poisson study pools the defects over the units and reports \
                     the defects per unit (DPU) and its PPM.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One column holds the defectives (binomial) or the defects (Poisson) per \
                     subgroup. Supply the inspected units in a second column, or set a constant \
                     size. In the binomial study the defectives cannot exceed the inspected \
                     units. Rows with a missing count or size are excluded and counted. Both \
                     reports use an exact confidence interval at the chosen level.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A lot inspects 240 units in 12 subgroups and records 72 defectives: the \
                     average p is 3 %, or 30,000 PPM defective, and process Z is 1.88. For \
                     defects, 24 defects over 40 units give DPU 0.60 and 600,000 PPM defects.",
                ));
                egui::Grid::new("attrcap_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("defectives");
                        ui.strong("inspected");
                        ui.end_row();
                        for (defectives, inspected) in [
                            ("6", "20"),
                            ("5", "20"),
                            ("7", "20"),
                            ("6", "20"),
                            ("6", "20"),
                            ("6", "20"),
                        ] {
                            ui.monospace(defectives);
                            ui.monospace(inspected);
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
        self.show_attrcap_help = open && !close;
    }

    pub fn open_g_chart_dialog(&mut self) {
        self.g_chart_counts = self.selected.clone().unwrap_or_default();
        self.g_chart_times = String::new();
        self.show_g_chart_dialog = true;
    }
    pub fn run_g_chart_dialog(&mut self) {
        if self.g_chart_counts.is_empty() {
            self.set_error("Select the column with the number of events");
            return;
        }
        let times = if self.g_chart_times.is_empty() {
            None
        } else {
            Some(self.g_chart_times.clone())
        };
        let spec = AnalysisSpec::rare_events(self.g_chart_counts.clone(), times);
        let label = format!("G Chart (Rare Events): {}", self.g_chart_counts);
        self.execute_spec(spec, label);
        self.show_g_chart_dialog = false;
    }
    pub(super) fn g_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_g_chart_dialog {
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
        let mut open = self.show_g_chart_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("G Chart (Rare Events)"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Events:");
                    egui::ComboBox::from_id_salt("g_chart_counts")
                        .selected_text(if self.g_chart_counts.is_empty() {
                            "—".to_owned()
                        } else {
                            self.g_chart_counts.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.g_chart_counts,
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
                    let label = ui.help_label("Times:");
                    egui::ComboBox::from_id_salt("g_chart_times")
                        .selected_text(if self.g_chart_times.is_empty() {
                            "— (sequence)".to_owned()
                        } else {
                            self.g_chart_times.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.g_chart_times,
                                String::new(),
                                "— (sequence)",
                            );
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.g_chart_times,
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
                    "The number of events between the failures of a unit; the limits are the \
                     Poisson limits.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_g_chart_dialog = open && !run;
        if run {
            self.run_g_chart_dialog();
        }
    }
    /// Opens the T chart dialog with the selected column.
    pub fn open_t_chart_dialog(&mut self) {
        self.t_chart_times = self.selected.clone().unwrap_or_default();
        self.show_t_chart_dialog = true;
    }

    /// Runs the T chart selected in the dialog.
    pub fn run_t_chart_dialog(&mut self) {
        if self.t_chart_times.is_empty() {
            self.set_error("Select the column with the time between events");
            return;
        }
        let spec = AnalysisSpec::t_chart(self.t_chart_times.clone())
            .with_t_chart_model(self.t_chart_model);
        let label = format!("T Chart (Rare Events): {}", self.t_chart_times);
        self.execute_spec(spec, label);
        self.show_t_chart_dialog = false;
    }

    /// T chart dialog: intervals between rare events.
    pub(super) fn t_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t_chart_dialog {
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
        let mut open = self.show_t_chart_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("T Chart (Rare Events)"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("t_chart_times")
                        .selected_text(if self.t_chart_times.is_empty() {
                            "—".to_owned()
                        } else {
                            self.t_chart_times.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.t_chart_times,
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
                    let label = ui.help_label("Model:");
                    egui::ComboBox::from_id_salt("t_chart_model")
                        .selected_text(self.t_chart_model.name())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for model in [
                                fp_core::rare::TChartDistribution::Exponential,
                                fp_core::rare::TChartDistribution::Gamma,
                            ] {
                                ui.selectable_value(&mut self.t_chart_model, model, model.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "The time between successive events; the limits are the 0.135 % and \
                     99.865 % quantiles of the fitted distribution and the centre line is its \
                     median.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About the T chart…"))
                        .clicked()
                    {
                        self.show_t_chart_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.t_chart_times.is_empty(),
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
        self.show_t_chart_dialog = open && !run && !cancel;
        if run {
            self.run_t_chart_dialog();
        }
    }

    /// Help page of the T chart dialog.
    pub(super) fn t_chart_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_t_chart_help {
            return;
        }
        let mut open = self.show_t_chart_help;
        let mut close = false;
        dialog_window(self.catalog().label("T Chart help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The T chart monitors the time between successive events. When the events \
                     are rare the intervals are skewed, so the limits are the 0.135 % and \
                     99.865 % quantiles of the fitted distribution and the centre line is its \
                     median. A short interval means the events are clustering; a long one means \
                     the process slowed.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column holds the positive time between successive events, in \
                     order. Rows with a missing time are excluded and counted. The exponential \
                     model uses the mean interval; the gamma model estimates the shape and scale \
                     from the mean and the n−1 variance, so it needs intervals that are not all \
                     equal.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "Five failures arrive 10, 12, 8, 15 and 9 days apart. The mean is 10.8 days, \
                     so the median is 7.49 days and the exponential limits are 0.015 and 71.36 \
                     days. Choose the gamma model when the intervals vary less than an \
                     exponential distribution would.",
                ));
                egui::Grid::new("t_chart_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("interval");
                        ui.strong("days");
                        ui.end_row();
                        for (interval, days) in [
                            ("1", "10"),
                            ("2", "12"),
                            ("3", "8"),
                            ("4", "15"),
                            ("5", "9"),
                        ] {
                            ui.monospace(interval);
                            ui.monospace(days);
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
        self.show_t_chart_help = open && !close;
    }

    pub fn open_tolerance_dialog(&mut self) {
        self.tolerance_column = self.selected.clone().unwrap_or_default();
        self.show_tolerance_dialog = true;
    }
    pub(super) fn tolerance_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_tolerance_dialog {
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
        let mut open = self.show_tolerance_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Tolerance Interval"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("tolerance_column")
                        .selected_text(if self.tolerance_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.tolerance_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.tolerance_column,
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
                    let label = ui.help_label("Proportion of the population:");
                    ui.add(
                        egui::DragValue::new(&mut self.tolerance_proportion)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.tolerance_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Method:");
                    ui.selectable_value(&mut self.tolerance_free, false, "Normal");
                    ui.selectable_value(&mut self.tolerance_free, true, "Distribution-free");
                });
                ui.small(if self.tolerance_free {
                    "Order statistics: no assumption about the shape of the population, but \
                         it needs a larger sample (93 observations for 95 %/95 %)."
                } else {
                    "The interval covers the chosen proportion of the population with the \
                         chosen confidence; the population must be normal."
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_tolerance_dialog = open && !run;
        if run {
            self.run_tolerance_dialog();
        }
    }
    pub fn open_sampling_dialog(&mut self) {
        self.sampling_plan_mode = false;
        self.show_sampling_dialog = true;
    }
    pub fn run_sampling_dialog(&mut self) {
        let outcome = if self.sampling_plan_mode {
            fp_core::sampling::analyze_plan(
                (self.sampling_aql / 100.0).clamp(0.0001, 0.99),
                (self.sampling_ltpd / 100.0).clamp(0.0002, 0.999),
                (self.sampling_alpha / 100.0).clamp(0.001, 0.5),
                (self.sampling_beta / 100.0).clamp(0.001, 0.5),
                (self.sampling_lot > 0).then_some(self.sampling_lot),
            )
        } else {
            fp_core::sampling::analyze(
                (self.sampling_lot > 0).then_some(self.sampling_lot),
                self.sampling_sample,
                self.sampling_accept,
            )
        };
        match outcome {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.adopt_plot(out.plot);
                self.status = "Acceptance Sampling".to_owned();
                self.error = None;
                self.show_sampling_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn sampling_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sampling_dialog {
            return;
        }
        let mut open = self.show_sampling_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Acceptance Sampling"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.sampling_plan_mode, false, "Evaluate a plan");
                    ui.selectable_value(&mut self.sampling_plan_mode, true, "Plan a scheme");
                });
                if self.sampling_plan_mode {
                    ui.horizontal(|ui| {
                        let aql = ui.help_label("AQL %:");
                        ui.add(
                            egui::DragValue::new(&mut self.sampling_aql)
                                .range(0.01..=20.0)
                                .speed(0.1),
                        )
                        .labelled_by(aql.id)
                        .hover_from(&aql);
                        let ltpd = ui.help_label("LTPD %:");
                        ui.add(
                            egui::DragValue::new(&mut self.sampling_ltpd)
                                .range(0.1..=50.0)
                                .speed(0.1),
                        )
                        .labelled_by(ltpd.id)
                        .hover_from(&ltpd);
                    });
                    ui.horizontal(|ui| {
                        let producer = ui.help_label("Producer risk %:");
                        ui.add(egui::DragValue::new(&mut self.sampling_alpha).range(0.5..=50.0))
                            .labelled_by(producer.id)
                            .hover_from(&producer);
                        let consumer = ui.help_label("Consumer risk %:");
                        ui.add(egui::DragValue::new(&mut self.sampling_beta).range(0.5..=50.0))
                            .labelled_by(consumer.id)
                            .hover_from(&consumer);
                    });
                    ui.small("The smallest single-sampling scheme that meets both risks.");
                } else {
                    ui.horizontal(|ui| {
                        let sample = ui.help_label("Sample size:");
                        ui.add(egui::DragValue::new(&mut self.sampling_sample).range(1..=5000))
                            .labelled_by(sample.id)
                            .hover_from(&sample);
                        let accept = ui.help_label("Acceptance:");
                        ui.add(egui::DragValue::new(&mut self.sampling_accept).range(0..=500))
                            .labelled_by(accept.id)
                            .hover_from(&accept);
                    });
                    ui.horizontal(|ui| {
                        let lot = ui.help_label("Lot size:");
                        ui.add(egui::DragValue::new(&mut self.sampling_lot).range(0..=10_000_000))
                            .labelled_by(lot.id)
                            .hover_from(&lot);
                        ui.small("0 = large lot (binomial)");
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_sampling_dialog = open && !run;
        if run {
            self.run_sampling_dialog();
        }
    }
    pub fn open_pareto_dialog(&mut self) {
        self.pareto_column = self.selected.clone().unwrap_or_default();
        self.pareto_weight.clear();
        self.show_pareto_dialog = true;
    }
    pub fn run_pareto_dialog(&mut self) {
        if self.pareto_column.is_empty() {
            self.set_error("Select the column to count");
            return;
        }
        let spec = if self.pareto_weight.is_empty() {
            AnalysisSpec::pareto(self.pareto_column.clone())
        } else {
            AnalysisSpec::pareto_weighted(self.pareto_column.clone(), self.pareto_weight.clone())
        };
        let label = if self.pareto_weight.is_empty() {
            format!("Pareto: {}", self.pareto_column)
        } else {
            format!(
                "Pareto: {} weighted by {}",
                self.pareto_column, self.pareto_weight
            )
        };
        self.execute_spec(spec, label);
        self.show_pareto_dialog = false;
    }
    pub(super) fn pareto_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pareto_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_pareto_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Pareto Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("pareto_column")
                        .selected_text(if self.pareto_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.pareto_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.pareto_column,
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
                    let label = ui.help_label("Weight column (optional):");
                    egui::ComboBox::from_id_salt("pareto_weight")
                        .selected_text(if self.pareto_weight.is_empty() {
                            "— none —".to_owned()
                        } else {
                            self.pareto_weight.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.pareto_weight, String::new(), "— none —");
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.pareto_weight,
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
                    "The categories are counted and sorted by frequency; with a weight column \
                     each row adds that value instead of one.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_pareto_dialog = open && !run;
        if run {
            self.run_pareto_dialog();
        }
    }
    pub fn open_ordinal_dialog(&mut self) {
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
        self.ordinal_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.ordinal_predictors = numeric
            .iter()
            .filter(|name| **name != self.ordinal_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_ordinal_dialog = true;
    }
    pub fn run_ordinal_dialog(&mut self) {
        if self.ordinal_response.is_empty() {
            self.set_error("Select the ordered response column");
            return;
        }
        let predictors: Vec<String> = self
            .ordinal_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.ordinal_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::ordinal(self.ordinal_response.clone(), predictors);
        let label = format!("Ordinal Regression: {}", self.ordinal_response);
        self.execute_spec(spec, label);
        self.show_ordinal_dialog = false;
    }
    pub(super) fn ordinal_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_ordinal_dialog {
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
        let mut open = self.show_ordinal_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Ordinal Logistic Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (codes):");
                    egui::ComboBox::from_id_salt("ordinal_response")
                        .selected_text(if self.ordinal_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.ordinal_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.ordinal_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.ordinal_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "The response needs at least three ordered integer codes; the model uses \
                     proportional odds.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_ordinal_dialog = open && !run;
        if run {
            self.run_ordinal_dialog();
        }
    }
    pub fn open_poisson_dialog(&mut self) {
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
        self.poisson_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.poisson_predictors = numeric
            .iter()
            .filter(|name| **name != self.poisson_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_poisson_dialog = true;
    }
    pub fn run_poisson_dialog(&mut self) {
        if self.poisson_response.is_empty() {
            self.set_error("Select the count response column");
            return;
        }
        let predictors: Vec<String> = self
            .poisson_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.poisson_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::poisson(self.poisson_response.clone(), predictors);
        let label = format!("Poisson Regression: {}", self.poisson_response);
        self.execute_spec(spec, label);
        self.show_poisson_dialog = false;
    }
    pub(super) fn poisson_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_poisson_dialog {
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
        let mut open = self.show_poisson_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Poisson Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (counts):");
                    egui::ComboBox::from_id_salt("poisson_response")
                        .selected_text(if self.poisson_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.poisson_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.poisson_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.poisson_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Comma-separated numeric predictors; the response must be a count.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_poisson_dialog = open && !run;
        if run {
            self.run_poisson_dialog();
        }
    }
    pub fn open_logistic_dialog(&mut self) {
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
        self.logistic_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.logistic_predictors = numeric
            .iter()
            .filter(|name| **name != self.logistic_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_logistic_dialog = true;
    }
    pub fn run_logistic_dialog(&mut self) {
        if self.logistic_response.is_empty() {
            self.set_error("Select the binary response column");
            return;
        }
        let predictors: Vec<String> = self
            .logistic_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.logistic_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::logistic(self.logistic_response.clone(), predictors);
        let label = format!("Logistic Regression: {}", self.logistic_response);
        self.execute_spec(spec, label);
        self.show_logistic_dialog = false;
    }
    pub(super) fn logistic_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_logistic_dialog {
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
        let mut open = self.show_logistic_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Binary Logistic Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (0/1):");
                    egui::ComboBox::from_id_salt("logistic_response")
                        .selected_text(if self.logistic_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.logistic_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.logistic_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.logistic_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Comma-separated predictors; the response must be 0 or 1. Text, boolean \
                     and date predictors are dummy coded with the first level as the reference.",
                );
                let columns: Vec<String> = self
                    .data
                    .as_ref()
                    .map(|df| {
                        df.columns()
                            .iter()
                            .map(|column| column.name().to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                if !columns.is_empty() {
                    ui.small(format!("Columns: {}", columns.join(", ")));
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_logistic_dialog = open && !run;
        if run {
            self.run_logistic_dialog();
        }
    }
}
