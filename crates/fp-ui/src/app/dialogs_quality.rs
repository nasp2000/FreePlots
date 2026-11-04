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

        let mut open = self.show_cap_dialog;
        let mut close = false;
        let mut pending: Option<AnalysisSpec> = None;
        dialog_window(self.catalog().label("Capability Analysis"))
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
                    ui.add(egui::DragValue::new(&mut self.cap_subgroup).range(1..=100))
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
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        self.cap_distribution == DistributionKind::Normal,
                        egui::Checkbox::new(&mut self.cap_box_cox, "Box-Cox transformation"),
                    )
                    .on_hover_text(checkbox_help("Box-Cox transformation"));
                    ui.small("positive data; Normal model only");
                });
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
                                                    && self.cap_distribution
                                                        == DistributionKind::Normal,
                                            )
                                            .with_capability_stability(self.cap_require_stability);
                                    if let AnalysisSpec::Capability {
                                        subgroup: size,
                                        confidence,
                                        ..
                                    } = &mut spec
                                    {
                                        *size = self.cap_subgroup;
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
                                ui.selectable_value(&mut self.chart_kind, kind, kind.name());
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
                    );
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
        self.attr_kind = AttributeChartKind::P;
        self.attr_sizes = None;
        self.attr_size = "100".to_owned();
        self.show_attr_dialog = true;
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
    pub fn open_tolerance_dialog(&mut self) {
        self.tolerance_column = self.selected.clone().unwrap_or_default();
        self.tolerance_proportion = 95.0;
        self.tolerance_confidence = 95.0;
        self.tolerance_free = false;
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
                self.show_plot = out.plot.is_some();
                self.plot = out.plot;
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
