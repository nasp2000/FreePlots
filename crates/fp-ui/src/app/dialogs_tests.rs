// SPDX-License-Identifier: MIT
//! Hypothesis tests and grouped descriptives: t, equivalence, rank, proportions,
//! power, crosstab, two-way ANOVA, interval plots and partial correlation.

use super::*;

impl FreePlotsApp {
    pub fn open_t_test_dialog(&mut self, kind: TTestKind) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let names: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if names.is_empty() {
            self.set_error("No numeric column available");
            return;
        }
        self.t_kind = kind;
        self.t_first = self
            .selected
            .clone()
            .filter(|name| names.contains(name))
            .unwrap_or_else(|| names[0].clone());
        self.t_second = names
            .iter()
            .find(|name| **name != self.t_first)
            .cloned()
            .unwrap_or_default();
        self.t_null = "0".to_owned();
        // The alternative, pooling and confidence are preferences now.
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
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
        self.show_t_dialog = true;
    }
    pub fn run_t_test_dialog(&mut self) {
        if self.t_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        let spec = match self.t_kind {
            TTestKind::OneSample => {
                let Ok(null_mean) = self.t_null.trim().parse::<f64>() else {
                    self.set_error("Null mean must be a number");
                    return;
                };
                AnalysisSpec::one_sample_t(self.t_first.clone(), null_mean)
            }
            TTestKind::TwoSample | TTestKind::Paired => {
                if self.t_second.is_empty() {
                    self.set_error("Select the second numeric column");
                    return;
                }
                if self.t_second == self.t_first {
                    self.set_error("Select two different columns");
                    return;
                }
                if self.t_kind == TTestKind::TwoSample {
                    AnalysisSpec::two_sample_t(self.t_first.clone(), self.t_second.clone())
                        .with_pooled(self.t_pooled)
                } else {
                    AnalysisSpec::paired_t(self.t_first.clone(), self.t_second.clone())
                }
            }
        }
        .with_alternative(self.t_alternative)
        .with_t_confidence((self.t_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.t_kind.name(),
            self.t_first,
            self.t_second
        );
        self.execute_spec(spec, label);
        self.show_t_dialog = false;
    }
    pub(super) fn t_test_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t_dialog {
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
        let mut open = self.show_t_dialog;
        let mut run = false;
        dialog_window(self.t_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("t_first_column")
                        .selected_text(if self.t_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.t_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.t_first, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.t_kind != TTestKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("t_second_column")
                            .selected_text(if self.t_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.t_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.t_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.t_kind == TTestKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null mean:");
                        ui.text_edit_singleline(&mut self.t_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Alternative:");
                    egui::ComboBox::from_id_salt("t_alternative")
                        .selected_text(self.t_alternative.as_str())
                        .show_ui(ui, |ui| {
                            for alternative in [
                                Alternative::TwoSided,
                                Alternative::Less,
                                Alternative::Greater,
                            ] {
                                ui.selectable_value(
                                    &mut self.t_alternative,
                                    alternative,
                                    alternative.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.t_kind == TTestKind::TwoSample {
                    ui.help_checkbox(&mut self.t_pooled, "Assume equal variances (pooled)");
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.t_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(match self.t_kind {
                    TTestKind::OneSample => {
                        "H₀: µ = null mean; sample standard deviation uses n − 1."
                    }
                    TTestKind::TwoSample => {
                        "H₀: µ₁ − µ₂ = 0; Welch is the default, pooled is optional."
                    }
                    TTestKind::Paired => "H₀: mean difference = 0; only complete pairs are used.",
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_t_dialog = open && !run;
        if run {
            self.run_t_test_dialog();
        }
    }
    pub fn open_equivalence_dialog(&mut self, kind: EquivalenceKind) {
        self.equivalence_kind = kind;
        self.equivalence_first = self.selected.clone().unwrap_or_default();
        self.equivalence_second = String::new();
        // The reference, margin and confidence are preferences now.
        self.show_equivalence_dialog = true;
    }
    pub fn run_equivalence_dialog(&mut self) {
        if self.equivalence_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        let Ok(margin) = self.equivalence_margin.trim().parse::<f64>() else {
            self.set_error("The equivalence margin must be a number");
            return;
        };
        if margin <= 0.0 {
            self.set_error("The equivalence margin must be positive");
            return;
        }
        let spec = match self.equivalence_kind {
            EquivalenceKind::OneSample => {
                let Ok(null_mean) = self.equivalence_null.trim().parse::<f64>() else {
                    self.set_error("The documented behaviour mean must be a number");
                    return;
                };
                AnalysisSpec::one_sample_equivalence(
                    self.equivalence_first.clone(),
                    null_mean,
                    margin,
                )
            }
            EquivalenceKind::TwoSample | EquivalenceKind::Paired => {
                if self.equivalence_second.is_empty() {
                    self.set_error("Select the second numeric column");
                    return;
                }
                if self.equivalence_second == self.equivalence_first {
                    self.set_error("Select two different columns");
                    return;
                }
                if self.equivalence_kind == EquivalenceKind::TwoSample {
                    AnalysisSpec::two_sample_equivalence(
                        self.equivalence_first.clone(),
                        self.equivalence_second.clone(),
                        margin,
                    )
                } else {
                    AnalysisSpec::paired_equivalence(
                        self.equivalence_first.clone(),
                        self.equivalence_second.clone(),
                        margin,
                    )
                }
            }
        }
        .with_equivalence_confidence((self.equivalence_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.equivalence_kind.name(),
            self.equivalence_first,
            self.equivalence_second
        );
        self.execute_spec(spec, label);
        self.show_equivalence_dialog = false;
    }
    pub(super) fn equivalence_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_equivalence_dialog {
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
        let mut open = self.show_equivalence_dialog;
        let mut run = false;
        dialog_window(self.equivalence_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("equivalence_first_column")
                        .selected_text(if self.equivalence_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.equivalence_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.equivalence_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.equivalence_kind != EquivalenceKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("equivalence_second_column")
                            .selected_text(if self.equivalence_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.equivalence_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.equivalence_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.equivalence_kind == EquivalenceKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Reference mean:");
                        ui.text_edit_singleline(&mut self.equivalence_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Margin:");
                    ui.text_edit_singleline(&mut self.equivalence_margin)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.equivalence_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "H₀: |effect| ≥ margin; the test declares equivalence when the (1 − 2α) \
                     interval is inside the margin.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_equivalence_dialog = open && !run;
        if run {
            self.run_equivalence_dialog();
        }
    }
    pub fn open_rank_test_dialog(&mut self, kind: RankTestKind) {
        self.rank_kind = kind;
        self.rank_first = self.selected.clone().unwrap_or_default();
        self.rank_second = String::new();
        self.rank_factor = String::new();
        self.rank_alternative = Alternative::TwoSided;
        self.rank_extra_columns = String::new();
        self.show_rank_dialog = true;
    }
    pub fn run_rank_test_dialog(&mut self) {
        if self.rank_kind.uses_factor() {
            if self.rank_first.is_empty() {
                self.set_error("Select the response column");
                return;
            }
            if self.rank_factor.is_empty() {
                self.set_error("Select the factor column");
                return;
            }
            let spec = if self.rank_kind == RankTestKind::MoodsMedian {
                AnalysisSpec::moods_median(self.rank_first.clone(), self.rank_factor.clone())
            } else {
                AnalysisSpec::kruskal_wallis(self.rank_first.clone(), self.rank_factor.clone())
            };
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_kind == RankTestKind::SignTest {
            if self.rank_first.is_empty() {
                self.set_error("Select the column to test");
                return;
            }
            // The documented behaviour tests the median against 0 unless the user
            // types another value.
            let median = if self.rank_median.trim().is_empty() {
                0.0
            } else {
                match self.rank_median.trim().parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => {
                        self.set_error("The hypothesised median must be a number");
                        return;
                    }
                }
            };
            let spec = AnalysisSpec::sign_test(self.rank_first.clone(), median);
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_kind == RankTestKind::Friedman {
            if self.rank_first.is_empty() {
                self.set_error("Select the first column");
                return;
            }
            let mut columns = vec![self.rank_first.clone()];
            for name in self.rank_extra_columns.split(',') {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                columns.push(name.to_owned());
            }
            if columns.len() < 2 {
                self.set_error("Add at least one more column");
                return;
            }
            let spec = AnalysisSpec::friedman(columns);
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        if self.rank_second.is_empty() {
            self.set_error("Select the second numeric column");
            return;
        }
        if self.rank_second == self.rank_first {
            self.set_error("Select two different columns");
            return;
        }
        let spec = if self.rank_kind == RankTestKind::MannWhitney {
            AnalysisSpec::mann_whitney(self.rank_first.clone(), self.rank_second.clone())
        } else {
            AnalysisSpec::wilcoxon_signed_rank(self.rank_first.clone(), self.rank_second.clone())
        }
        .with_alternative(self.rank_alternative);
        let label = format!(
            "{}: {} / {}",
            self.rank_kind.name(),
            self.rank_first,
            self.rank_second
        );
        self.execute_spec(spec, label);
        self.show_rank_dialog = false;
    }
    pub(super) fn rank_test_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_rank_dialog {
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
        let mut open = self.show_rank_dialog;
        let mut run = false;
        dialog_window(self.rank_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(if self.rank_kind == RankTestKind::KruskalWallis {
                        "Response:"
                    } else {
                        "First column:"
                    });
                    egui::ComboBox::from_id_salt("rank_first_column")
                        .selected_text(if self.rank_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.rank_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.rank_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.rank_kind == RankTestKind::Friedman {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("More columns:");
                        ui.text_edit_singleline(&mut self.rank_extra_columns)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.rank_kind.uses_factor() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Factor:");
                        egui::ComboBox::from_id_salt("rank_factor_column")
                            .selected_text(if self.rank_factor.is_empty() {
                                "—".to_owned()
                            } else {
                                self.rank_factor.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &self
                                    .data
                                    .as_ref()
                                    .map(|df| {
                                        df.columns()
                                            .iter()
                                            .map(|column| column.name().to_owned())
                                            .collect::<Vec<String>>()
                                    })
                                    .unwrap_or_default()
                                {
                                    ui.selectable_value(
                                        &mut self.rank_factor,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                } else if self.rank_kind != RankTestKind::Friedman {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("rank_second_column")
                            .selected_text(if self.rank_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.rank_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.rank_second,
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
                        let label = ui.help_label("Alternative:");
                        egui::ComboBox::from_id_salt("rank_alternative")
                            .selected_text(self.rank_alternative.as_str())
                            .show_ui(ui, |ui| {
                                for alternative in [
                                    Alternative::TwoSided,
                                    Alternative::Less,
                                    Alternative::Greater,
                                ] {
                                    ui.selectable_value(
                                        &mut self.rank_alternative,
                                        alternative,
                                        alternative.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small(match self.rank_kind {
                    RankTestKind::MannWhitney => {
                        "H₀: the two samples come from the same distribution; the p-value uses the \
                         normal approximation."
                    }
                    RankTestKind::Wilcoxon => {
                        "H₀: the median difference is zero; only complete pairs are used."
                    }
                    RankTestKind::KruskalWallis => {
                        "H₀: the groups come from the same distribution; the p-value uses the \
                         chi-square approximation."
                    }
                    RankTestKind::Friedman => {
                        "H₀: the groups come from the same distribution; every row is a block."
                    }
                    RankTestKind::MoodsMedian => {
                        "H₀: the groups have the same median; the counts around the grand median \
                         are compared."
                    }
                    RankTestKind::SignTest => {
                        "H₀: the median equals the hypothesised value; the p-value is the exact \
                         two-sided binomial."
                    }
                });
                if self.rank_kind == RankTestKind::SignTest {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Hypothesised median:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rank_median)
                                .desired_width(120.0)
                                .hint_text("0"),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_rank_dialog = open && !run;
        if run {
            self.run_rank_test_dialog();
        }
    }
    /// Opens the runs test dialog with the selected column.
    pub fn open_runs_dialog(&mut self) {
        self.runs_column = self.selected.clone().unwrap_or_default();
        self.runs_reference.clear();
        self.show_runs_dialog = true;
    }

    /// Runs the Wald–Wolfowitz test with the dialog settings.
    pub fn run_runs_dialog(&mut self) {
        if self.runs_column.is_empty() {
            self.set_error("Select the column to test");
            return;
        }
        let reference = match self.runs_reference.trim() {
            "" => None,
            raw => match raw.parse::<f64>() {
                Ok(value) if value.is_finite() => Some(value),
                _ => {
                    self.set_error("The comparison value must be a number");
                    return;
                }
            },
        };
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::runs::analyze(&df, &self.runs_column, reference) {
            Ok(result) => {
                self.append_session(&format!("{}\n", result.to_session_text()));
                self.status = format!("Runs test: {}", self.runs_column);
                self.show_runs_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Runs test dialog: the column and the optional comparison value.
    pub(super) fn runs_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_runs_dialog {
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
        let mut open = self.show_runs_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Runs Test"))
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("runs_column")
                        .selected_text(if self.runs_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.runs_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.runs_column,
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
                    let label = ui.help_label("Comparison value:");
                    ui.text_edit_singleline(&mut self.runs_reference)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "Blank uses the mean of the observations. Values above the comparison value \
                     and values below or equal to it form the runs.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About the runs test…"))
                        .clicked()
                    {
                        self.show_runs_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.runs_column.is_empty(),
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
        self.show_runs_dialog = open && !run && !cancel;
        if run {
            self.run_runs_dialog();
        }
    }

    /// Help page of the runs test.
    pub(super) fn runs_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_runs_help {
            return;
        }
        let mut open = self.show_runs_help;
        let mut close = false;
        dialog_window(self.catalog().label("Runs Test help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The runs test checks whether a sequence is random. A run is a group of \
                     consecutive observations on the same side of the comparison value. Too few \
                     runs suggest clustering; too many suggest an alternating pattern. The \
                     p-value is two-sided.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "One numeric column, read in worksheet order. The comparison value is \
                     optional: blank uses the mean of the observations. Values above it and \
                     values below or equal to it form the two sides, so a value equal to the \
                     comparison value joins the lower side. Rows with a missing value are \
                     removed and counted. The normal approximation is reliable with at least 11 \
                     observations on each side.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A die is rolled 25 times and the scores are compared with 3.5: 4 and 6 are \
                     above, 3, 2 and 1 below. The sequence has 10 runs against 13.32 expected, \
                     Z = −1.38 and p = 0.168, so there is no evidence against randomness. An \
                     alternating 1, 10, 1, 10 sequence gives 20 runs and a very small p-value.",
                ));
                egui::Grid::new("runs_help_example")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("roll");
                        ui.strong("score");
                        ui.end_row();
                        for (roll, score) in [
                            ("1", "4"),
                            ("2", "6"),
                            ("3", "3"),
                            ("4", "2"),
                            ("5", "2"),
                            ("6", "1"),
                        ] {
                            ui.monospace(roll);
                            ui.monospace(score);
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
        self.show_runs_help = open && !close;
    }

    pub fn open_test2_dialog(&mut self, kind: Test2Kind) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let names: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if names.is_empty() {
            self.set_error("No numeric column available");
            return;
        }
        self.test2_kind = kind;
        self.test2_first = self
            .selected
            .clone()
            .filter(|name| names.contains(name))
            .unwrap_or_else(|| names[0].clone());
        self.test2_second = names
            .iter()
            .find(|name| **name != self.test2_first)
            .cloned()
            .unwrap_or_default();
        self.test2_null = match kind {
            Test2Kind::OneProportion => "0.5".to_owned(),
            Test2Kind::OneVariance => "1".to_owned(),
            _ => String::new(),
        };
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
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
        self.show_test2_dialog = true;
    }
    pub fn run_test2_dialog(&mut self) {
        if self.test2_first.is_empty() {
            self.set_error("Select the first column");
            return;
        }
        let proportion = matches!(
            self.test2_kind,
            Test2Kind::OneProportion | Test2Kind::TwoProportions
        );
        let success = if proportion {
            match self.test2_success.trim().parse::<f64>() {
                Ok(value) if value == 0.0 || value == 1.0 => value,
                _ => {
                    self.set_error("Success value must be 0 or 1");
                    return;
                }
            }
        } else {
            1.0
        };
        let spec = match self.test2_kind {
            Test2Kind::OneProportion => {
                let Ok(null_p) = self.test2_null.trim().parse::<f64>() else {
                    self.set_error("Null proportion must be a number");
                    return;
                };
                AnalysisSpec::one_proportion(self.test2_first.clone(), null_p)
                    .with_success(success)
                    .with_proportion_method(self.test2_method)
            }
            Test2Kind::TwoProportions => {
                if self.test2_second.is_empty() || self.test2_second == self.test2_first {
                    self.set_error("Select two different columns");
                    return;
                }
                AnalysisSpec::two_proportions(self.test2_first.clone(), self.test2_second.clone())
                    .with_success(success)
            }
            Test2Kind::OneVariance => {
                let Ok(null_variance) = self.test2_null.trim().parse::<f64>() else {
                    self.set_error("Null variance must be a number");
                    return;
                };
                AnalysisSpec::one_variance(self.test2_first.clone(), null_variance)
            }
            Test2Kind::TwoVariances => {
                if self.test2_second.is_empty() || self.test2_second == self.test2_first {
                    self.set_error("Select two different columns");
                    return;
                }
                AnalysisSpec::two_variances(self.test2_first.clone(), self.test2_second.clone())
                    .with_variance_method(self.test2_variance_method)
            }
        }
        .with_alternative(self.test2_alternative)
        .with_test_confidence((self.test2_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.test2_kind.name(),
            self.test2_first,
            self.test2_second
        );
        self.execute_spec(spec, label);
        self.show_test2_dialog = false;
    }
    pub(super) fn test2_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_test2_dialog {
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
        let mut open = self.show_test2_dialog;
        let mut run = false;
        dialog_window(self.test2_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(410.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("test2_first")
                        .selected_text(if self.test2_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.test2_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.test2_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.test2_kind.needs_second() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("test2_second")
                            .selected_text(if self.test2_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.test2_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.test2_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                let proportion = matches!(
                    self.test2_kind,
                    Test2Kind::OneProportion | Test2Kind::TwoProportions
                );
                if proportion {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Success value (0/1):");
                        ui.text_edit_singleline(&mut self.test2_success)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.small("The selected columns must contain only 0 and 1.");
                }
                if matches!(self.test2_kind, Test2Kind::OneProportion) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null proportion:");
                        ui.text_edit_singleline(&mut self.test2_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Method:");
                        egui::ComboBox::from_id_salt("test2_method")
                            .selected_text(self.test2_method.as_str())
                            .show_ui(ui, |ui| {
                                for method in [ProportionMethod::Exact, ProportionMethod::Normal] {
                                    ui.selectable_value(
                                        &mut self.test2_method,
                                        method,
                                        method.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(self.test2_kind, Test2Kind::OneVariance) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null variance:");
                        ui.text_edit_singleline(&mut self.test2_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(self.test2_kind, Test2Kind::TwoVariances) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Method:");
                        egui::ComboBox::from_id_salt("test2_variance_method")
                            .selected_text(self.test2_variance_method.name())
                            .show_ui(ui, |ui| {
                                for method in [VarianceMethod::F, VarianceMethod::Bonett] {
                                    ui.selectable_value(
                                        &mut self.test2_variance_method,
                                        method,
                                        method.name(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Alternative:");
                    egui::ComboBox::from_id_salt("test2_alternative")
                        .selected_text(self.test2_alternative.as_str())
                        .show_ui(ui, |ui| {
                            for alternative in [
                                Alternative::TwoSided,
                                Alternative::Less,
                                Alternative::Greater,
                            ] {
                                ui.selectable_value(
                                    &mut self.test2_alternative,
                                    alternative,
                                    alternative.as_str(),
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
                        egui::DragValue::new(&mut self.test2_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(match self.test2_kind {
                    Test2Kind::OneProportion => {
                        "Exact binomial (Clopper–Pearson) or normal approximation (Wilson)."
                    }
                    Test2Kind::TwoProportions => {
                        "H₀: p₁ = p₂; pooled normal approximation, unpooled interval."
                    }
                    Test2Kind::OneVariance => {
                        "H₀: σ² = null variance; chi-square test with n − 1 df."
                    }
                    Test2Kind::TwoVariances => {
                        "H₀: σ₁² = σ₂²; F test (n₁−1, n₂−1 df) or Bonett's kurtosis-adjusted test."
                    }
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_test2_dialog = open && !run;
        if run {
            self.run_test2_dialog();
        }
    }
    pub fn open_power_dialog(&mut self) {
        // The procedure, alpha, target, alternative and groups are preferences.
        self.power_effect = "0.5".to_owned();
        self.power_null = "0.5".to_owned();
        self.power_n = "0".to_owned();
        self.show_power_dialog = true;
    }
    pub fn run_power_dialog(&mut self) {
        let Ok(effect) = self.power_effect.trim().parse::<f64>() else {
            self.set_error("The effect size must be a number");
            return;
        };
        let Ok(null_value) = self.power_null.trim().parse::<f64>() else {
            self.set_error("The null proportion must be a number");
            return;
        };
        let Ok(n) = self.power_n.trim().parse::<usize>() else {
            self.set_error("The sample size must be a whole number (0 solves for it)");
            return;
        };
        let spec = AnalysisSpec::power(self.power_procedure, effect, n)
            .with_null_proportion(null_value)
            .with_power((self.power_target / 100.0).clamp(0.001, 0.999))
            .with_power_alpha((self.power_alpha / 100.0).clamp(0.0001, 0.5))
            .with_power_groups(self.power_groups.max(2))
            .with_alternative(self.power_alternative);
        let label = format!("Power and Sample Size: {}", self.power_procedure.name());
        self.execute_spec(spec, label);
        self.show_power_dialog = false;
    }
    pub(super) fn power_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_power_dialog {
            return;
        }
        let mut open = self.show_power_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Power and Sample Size"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Test:");
                    egui::ComboBox::from_id_salt("power_procedure")
                        .selected_text(self.power_procedure.name())
                        .show_ui(ui, |ui| {
                            for procedure in [
                                PowerProcedure::OneSampleT,
                                PowerProcedure::TwoSampleT,
                                PowerProcedure::Proportion,
                                PowerProcedure::Correlation,
                                PowerProcedure::Anova,
                                PowerProcedure::Regression,
                            ] {
                                ui.selectable_value(
                                    &mut self.power_procedure,
                                    procedure,
                                    procedure.name(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.label(match self.power_procedure {
                        PowerProcedure::Proportion => "True proportion:",
                        PowerProcedure::Correlation => "Correlation:",
                        PowerProcedure::Anova => "Standardized difference:",
                        PowerProcedure::Regression => "Population R²:",
                        _ => "Effect size:",
                    });
                    ui.text_edit_singleline(&mut self.power_effect)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.power_procedure == PowerProcedure::Proportion {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null proportion:");
                        ui.text_edit_singleline(&mut self.power_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(
                    self.power_procedure,
                    PowerProcedure::Anova | PowerProcedure::Regression
                ) {
                    ui.horizontal(|ui| {
                        let label = ui.label(if self.power_procedure == PowerProcedure::Anova {
                            "Groups:"
                        } else {
                            "Predictors:"
                        });
                        ui.add(
                            egui::DragValue::new(&mut self.power_groups)
                                .range(2..=20)
                                .speed(0.5),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                if self.power_procedure != PowerProcedure::Correlation {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Sample size:");
                        ui.text_edit_singleline(&mut self.power_n)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Alternative:");
                        egui::ComboBox::from_id_salt("power_alternative")
                            .selected_text(self.power_alternative.as_str())
                            .show_ui(ui, |ui| {
                                for alternative in [
                                    Alternative::TwoSided,
                                    Alternative::Less,
                                    Alternative::Greater,
                                ] {
                                    ui.selectable_value(
                                        &mut self.power_alternative,
                                        alternative,
                                        alternative.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Significance:");
                    ui.add(
                        egui::DragValue::new(&mut self.power_alpha)
                            .range(0.1..=20.0)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Power:");
                    ui.add(
                        egui::DragValue::new(&mut self.power_target)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "A sample size of zero asks for the size that reaches the requested power; \
                     otherwise the power of that size is reported.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_power_dialog = open && !run;
        if run {
            self.run_power_dialog();
        }
    }
    pub fn open_crosstab_dialog(&mut self) {
        self.crosstab_row = self.selected.clone().unwrap_or_default();
        self.crosstab_column = String::new();
        self.show_crosstab_dialog = true;
    }
    pub fn run_crosstab_dialog(&mut self) {
        if self.crosstab_row.is_empty() || self.crosstab_column.is_empty() {
            self.set_error("Select the two variables");
            return;
        }
        if self.crosstab_row == self.crosstab_column {
            self.set_error("Select two different variables");
            return;
        }
        let spec = AnalysisSpec::chi_square_independence(
            self.crosstab_row.clone(),
            self.crosstab_column.clone(),
        );
        let label = format!(
            "Chi-Square Test: {} x {}",
            self.crosstab_row, self.crosstab_column
        );
        self.execute_spec(spec, label);
        self.show_crosstab_dialog = false;
    }
    pub(super) fn crosstab_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_crosstab_dialog {
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
        let mut open = self.show_crosstab_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Chi-Square Test for Independence"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Row:");
                    egui::ComboBox::from_id_salt("crosstab_row")
                        .selected_text(if self.crosstab_row.is_empty() {
                            "—".to_owned()
                        } else {
                            self.crosstab_row.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.crosstab_row,
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
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("crosstab_column")
                        .selected_text(if self.crosstab_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.crosstab_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.crosstab_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The two variables need at least two levels each.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_crosstab_dialog = open && !run;
        if run {
            self.run_crosstab_dialog();
        }
    }
    pub fn open_two_way_dialog(&mut self) {
        self.two_way_response = self.selected.clone().unwrap_or_default();
        self.two_way_first = String::new();
        self.two_way_second = String::new();
        self.two_way_interaction = false;
        self.show_two_way_dialog = true;
    }
    pub fn run_two_way_dialog(&mut self) {
        if self.two_way_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        if self.two_way_first.is_empty() || self.two_way_second.is_empty() {
            self.set_error("Select the two factor columns");
            return;
        }
        if self.two_way_first == self.two_way_second {
            self.set_error("Select two different factor columns");
            return;
        }
        let spec = AnalysisSpec::two_way_anova(
            self.two_way_response.clone(),
            self.two_way_first.clone(),
            self.two_way_second.clone(),
        )
        .with_interaction(self.two_way_interaction);
        let label = format!(
            "Two-Way ANOVA: {} by {} and {}",
            self.two_way_response, self.two_way_first, self.two_way_second
        );
        self.execute_spec(spec, label);
        self.show_two_way_dialog = false;
    }
    pub(super) fn two_way_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_two_way_dialog {
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
        let mut open = self.show_two_way_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Two-Way ANOVA"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("two_way_response")
                        .selected_text(if self.two_way_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.two_way_response,
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
                    let label = ui.help_label("Factor A:");
                    egui::ComboBox::from_id_salt("two_way_first")
                        .selected_text(if self.two_way_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.two_way_first,
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
                    let label = ui.help_label("Factor B:");
                    egui::ComboBox::from_id_salt("two_way_second")
                        .selected_text(if self.two_way_second.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_second.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.two_way_second,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(
                    &mut self.two_way_interaction,
                    "Fit the interaction (A×B)",
                )
                .on_hover_text(
                    "The documented command fits main effects only; the interaction adds its own term and error degrees of freedom.",
                );
                ui.small("The design must be complete and balanced.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_two_way_dialog = open && !run;
        if run {
            self.run_two_way_dialog();
        }
    }
    pub fn open_interval_dialog(&mut self) {
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
        self.interval_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.interval_factor = self
            .data
            .as_ref()
            .and_then(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .find(|name| *name != self.interval_response)
            })
            .unwrap_or_default();
        self.show_interval_dialog = true;
    }
    pub fn run_interval_dialog(&mut self) {
        if self.interval_response.is_empty() || self.interval_factor.is_empty() {
            self.set_error("Select the response and the factor columns");
            return;
        }
        if self.interval_response == self.interval_factor {
            self.set_error("The response and the factor must be different columns");
            return;
        }
        let spec = AnalysisSpec::interval_plot(
            self.interval_response.clone(),
            self.interval_factor.clone(),
        )
        .with_interval_confidence((self.interval_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!("Interval Plot: {}", self.interval_response);
        self.execute_spec(spec, label);
        self.show_interval_dialog = false;
    }
    pub(super) fn interval_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_interval_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_interval_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Interval Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Response:", &mut self.interval_response),
                    ("Factor:", &mut self.interval_factor),
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
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.interval_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Level means with their Student-t intervals and the grand mean.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_interval_dialog = open && !run;
        if run {
            self.run_interval_dialog();
        }
    }
    pub fn open_partial_dialog(&mut self) {
        self.partial_x = self.selected.clone().unwrap_or_default();
        self.partial_y = String::new();
        self.partial_control = String::new();
        self.show_partial_dialog = true;
    }
    pub fn run_partial_dialog(&mut self) {
        if self.partial_x.is_empty() || self.partial_y.is_empty() || self.partial_control.is_empty()
        {
            self.set_error("Select the two variables and the control");
            return;
        }
        if self.partial_x == self.partial_y
            || self.partial_x == self.partial_control
            || self.partial_y == self.partial_control
        {
            self.set_error("Select three different columns");
            return;
        }
        let spec = AnalysisSpec::partial_correlation(
            self.partial_x.clone(),
            self.partial_y.clone(),
            self.partial_control.clone(),
        );
        let label = format!(
            "Partial Correlation: {} and {} given {}",
            self.partial_x, self.partial_y, self.partial_control
        );
        self.execute_spec(spec, label);
        self.show_partial_dialog = false;
    }
    pub(super) fn partial_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_partial_dialog {
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
        let mut open = self.show_partial_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Partial Correlation"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                for (label, slot) in [
                    ("X:", &mut self.partial_x),
                    ("Y:", &mut self.partial_y),
                    ("Control:", &mut self.partial_control),
                ] {
                    ui.horizontal(|ui| {
                        let label_response = ui.label(label);
                        egui::ComboBox::from_id_salt(format!("partial_{label}"))
                            .selected_text(if slot.is_empty() {
                                "—".to_owned()
                            } else {
                                slot.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(slot, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label_response.id)
                            .hover_from(&label_response);
                    });
                }
                ui.small("The linear effect of the control is removed from both variables.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_partial_dialog = open && !run;
        if run {
            self.run_partial_dialog();
        }
    }
    pub fn open_describe_by_dialog(&mut self) {
        if let Some(column) = &self.selected {
            self.describe_column = column.clone();
        }
        self.describe_by = None;
        self.show_describe_dialog = true;
    }
    pub fn run_describe_by_dialog(&mut self) {
        if self.describe_column.is_empty() {
            self.set_error("Select the numeric column");
            return;
        }
        let Some(by) = self.describe_by.clone() else {
            self.set_error("Select the grouping column");
            return;
        };
        if by == self.describe_column {
            self.set_error("The grouping column must be different");
            return;
        }
        self.run_descriptive_statistics_by(self.describe_column.clone(), by);
        self.show_describe_dialog = false;
    }
    pub(super) fn describe_by_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_describe_dialog {
            return;
        }
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let numeric: Vec<String> = self
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
        let mut open = self.show_describe_dialog;
        let mut close = false;
        let mut run = false;
        dialog_window(self.catalog().label("Descriptive Statistics by Column"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Numeric column:");
                    egui::ComboBox::from_id_salt("describe_column")
                        .selected_text(if self.describe_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.describe_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.describe_column,
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
                    let label = ui.help_label("By column:");
                    egui::ComboBox::from_id_salt("describe_by")
                        .selected_text(self.describe_by.clone().unwrap_or_else(|| "—".to_owned()))
                        .show_ui(ui, |ui| {
                            for name in &all_columns {
                                ui.selectable_value(
                                    &mut self.describe_by,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("One statistics block is produced for each group level.");
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.describe_column.is_empty() && self.describe_by.is_some();
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
        self.show_describe_dialog = open && !close;
        if run {
            self.run_describe_by_dialog();
        }
    }
}
