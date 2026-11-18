// SPDX-License-Identifier: MIT
//! Data manipulation, file dialogs and the help windows: sort, stack, subset,
//! dates, the calculator, find/format, save/open and the About/settings/agents
//! windows.

use super::*;

impl FreePlotsApp {
    pub fn open_sort_dialog(&mut self) {
        self.sort_keys = self.selected.clone().unwrap_or_default();
        self.show_sort_dialog = true;
    }
    pub fn run_sort_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let mut keys = Vec::new();
        for part in self.sort_keys.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (name, direction) = match part.split_once(':') {
                Some((name, direction)) => {
                    let Some(direction) = fp_core::reshape::SortDirection::parse(direction.trim())
                    else {
                        self.set_error(format!("Unknown direction: {direction}"));
                        return;
                    };
                    (name.trim(), direction)
                }
                None => (part, fp_core::reshape::SortDirection::Ascending),
            };
            keys.push((name.to_owned(), direction));
        }
        if keys.is_empty() {
            self.set_error("List at least one sort column");
            return;
        }
        match fp_core::reshape::sort(&df, &keys) {
            Ok(sorted) => {
                self.replace_worksheet(sorted, "Sorted worksheet");
                self.show_sort_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn sort_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sort_dialog {
            return;
        }
        let mut open = self.show_sort_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Sort Worksheet"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Keys:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sort_keys)
                            .desired_width(260.0)
                            .hint_text("x1:desc,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Comma-separated columns; append :desc for a descending key.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_sort_dialog = open && !run;
        if run {
            self.run_sort_dialog();
        }
    }
    pub fn open_stack_dialog(&mut self) {
        self.stack_columns = self.selected.clone().unwrap_or_default();
        self.show_stack_dialog = true;
    }
    pub fn run_stack_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let columns: Vec<String> = self
            .stack_columns
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.is_empty() {
            self.set_error("List at least one column to stack");
            return;
        }
        match fp_core::reshape::stack(&df, &columns) {
            Ok(stacked) => {
                self.replace_worksheet(stacked, "Stacked worksheet");
                self.show_stack_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn stack_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_stack_dialog {
            return;
        }
        let mut open = self.show_stack_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stack Columns"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.stack_columns)
                            .desired_width(260.0)
                            .hint_text("C1,C2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("The result has the Value and Variable columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_stack_dialog = open && !run;
        if run {
            self.run_stack_dialog();
        }
    }
    pub fn open_unstack_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.unstack_value = self
            .selected
            .clone()
            .unwrap_or_else(|| names.first().cloned().unwrap_or_default());
        self.unstack_group = names
            .iter()
            .find(|name| **name != self.unstack_value)
            .cloned()
            .unwrap_or_default();
        self.show_unstack_dialog = true;
    }
    pub fn run_unstack_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::reshape::unstack(&df, &self.unstack_value, &self.unstack_group) {
            Ok(wide) => {
                self.replace_worksheet(wide, "Unstacked worksheet");
                self.show_unstack_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn unstack_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_unstack_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_unstack_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Unstack Column"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Value:", &mut self.unstack_value),
                    ("Group:", &mut self.unstack_group),
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
                ui.small("Each level of the group becomes a column of its values.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_unstack_dialog = open && !run;
        if run {
            self.run_unstack_dialog();
        }
    }
    pub fn open_date_diff_dialog(&mut self) {
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        self.date_diff_first = dates.first().cloned().unwrap_or_default();
        self.date_diff_second = dates.get(1).cloned().unwrap_or_default();
        self.date_diff_name = "days".to_owned();
        self.show_date_diff_dialog = true;
    }
    pub fn run_date_diff_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.date_diff_first.is_empty() || self.date_diff_second.is_empty() {
            self.set_error("Select the two date columns");
            return;
        }
        if self.date_diff_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        match fp_core::dates::add_difference_column(
            &mut df,
            &self.date_diff_first,
            &self.date_diff_second,
            self.date_diff_name.trim(),
        ) {
            Ok(()) => {
                let name = self.date_diff_name.trim().to_owned();
                self.set_data(df, "Date difference".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} − {} in days.\n",
                    self.date_diff_second, self.date_diff_first
                ));
                self.status = format!("Column {name} added");
                self.show_date_diff_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn date_diff_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_date_diff_dialog {
            return;
        }
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_date_diff_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Date Difference"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("First date:", &mut self.date_diff_first),
                    ("Second date:", &mut self.date_diff_second),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &dates {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.text_edit_singleline(&mut self.date_diff_name)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The result is second − first in days.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_date_diff_dialog = open && !run;
        if run {
            self.run_date_diff_dialog();
        }
    }
    pub fn open_date_shift_dialog(&mut self) {
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        self.date_shift_column = dates.first().cloned().unwrap_or_default();
        self.date_shift_days = 0;
        self.date_shift_name = "shifted".to_owned();
        self.show_date_shift_dialog = true;
    }
    pub fn run_date_shift_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.date_shift_column.is_empty() {
            self.set_error("Select the date column");
            return;
        }
        if self.date_shift_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        match fp_core::dates::add_shift_column(
            &mut df,
            &self.date_shift_column,
            self.date_shift_days,
            self.date_shift_name.trim(),
        ) {
            Ok(()) => {
                let name = self.date_shift_name.trim().to_owned();
                self.set_data(df, "Date shift".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} {} days.\n",
                    self.date_shift_column, self.date_shift_days
                ));
                self.status = format!("Column {name} added");
                self.show_date_shift_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn date_shift_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_date_shift_dialog {
            return;
        }
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_date_shift_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Add Days"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Date column:");
                    egui::ComboBox::from_id_salt("date_shift_column")
                        .selected_text(if self.date_shift_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.date_shift_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &dates {
                                ui.selectable_value(
                                    &mut self.date_shift_column,
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
                    let label = ui.help_label("Days:");
                    ui.add(
                        egui::DragValue::new(&mut self.date_shift_days)
                            .range(-100_000..=100_000)
                            .speed(1.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.text_edit_singleline(&mut self.date_shift_name)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Negative values move the dates backwards.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_date_shift_dialog = open && !run;
        if run {
            self.run_date_shift_dialog();
        }
    }
    pub fn open_subset_dialog(&mut self) {
        self.subset_column = self.selected.clone().unwrap_or_default();
        self.subset_op = fp_core::reshape::SubsetOp::Equal;
        self.subset_value.clear();
        self.show_subset_dialog = true;
    }
    pub fn run_subset_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.subset_column.is_empty() {
            self.set_error("Select the column of the condition");
            return;
        }
        match fp_core::reshape::subset(&df, &self.subset_column, self.subset_op, &self.subset_value)
        {
            Ok(filtered) => {
                self.replace_worksheet(filtered, "Subset worksheet");
                self.show_subset_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn subset_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_subset_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_subset_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Subset Worksheet"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("subset_column")
                        .selected_text(if self.subset_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.subset_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.subset_column,
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
                    ui.help_label("Condition:");
                    for op in [
                        fp_core::reshape::SubsetOp::Equal,
                        fp_core::reshape::SubsetOp::NotEqual,
                        fp_core::reshape::SubsetOp::Less,
                        fp_core::reshape::SubsetOp::LessOrEqual,
                        fp_core::reshape::SubsetOp::Greater,
                        fp_core::reshape::SubsetOp::GreaterOrEqual,
                        fp_core::reshape::SubsetOp::Contains,
                    ] {
                        ui.selectable_value(&mut self.subset_op, op, op.symbol());
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Value:");
                    ui.add(egui::TextEdit::singleline(&mut self.subset_value).desired_width(180.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Numeric comparisons are used when both sides are numbers.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_subset_dialog = open && !run;
        if run {
            self.run_subset_dialog();
        }
    }
    pub fn open_timeseries_dialog(&mut self) {
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
        self.timeseries_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.show_timeseries_dialog = true;
    }
    pub fn run_timeseries_dialog(&mut self) {
        if self.timeseries_column.is_empty() {
            self.set_error("Select the series column");
            return;
        }
        let spec = match self.timeseries_kind {
            TimeSeriesKind::MovingAverage => {
                AnalysisSpec::moving_average(self.timeseries_column.clone(), self.timeseries_window)
            }
            TimeSeriesKind::Decomposition => AnalysisSpec::decomposition(
                self.timeseries_column.clone(),
                self.timeseries_period,
                self.timeseries_model,
            ),
            TimeSeriesKind::ExponentialSmoothing => AnalysisSpec::exponential_smoothing(
                self.timeseries_column.clone(),
                self.timeseries_season,
                self.timeseries_horizon,
            ),
        };
        let label = format!(
            "{}: {}",
            self.timeseries_kind.name(),
            self.timeseries_column
        );
        self.execute_spec(spec, label);
        self.show_timeseries_dialog = false;
    }
    pub(super) fn timeseries_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_timeseries_dialog {
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
        let mut open = self.show_timeseries_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Time Series"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("timeseries_column")
                        .selected_text(if self.timeseries_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.timeseries_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.timeseries_column,
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
                    ui.help_label("Method:");
                    for (kind, label) in [
                        (TimeSeriesKind::MovingAverage, "Moving average"),
                        (TimeSeriesKind::Decomposition, "Decomposition"),
                        (TimeSeriesKind::ExponentialSmoothing, "Smoothing"),
                    ] {
                        ui.selectable_value(&mut self.timeseries_kind, kind, label);
                    }
                });
                match self.timeseries_kind {
                    TimeSeriesKind::MovingAverage => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Window:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_window).range(2..=100),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                    }
                    TimeSeriesKind::Decomposition => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Period:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_period).range(2..=365),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                        ui.horizontal(|ui| {
                            ui.help_label("Model:");
                            ui.selectable_value(
                                &mut self.timeseries_model,
                                fp_core::timeseries::DecompositionModel::Additive,
                                "Additive",
                            );
                            ui.selectable_value(
                                &mut self.timeseries_model,
                                fp_core::timeseries::DecompositionModel::Multiplicative,
                                "Multiplicative",
                            );
                        });
                    }
                    TimeSeriesKind::ExponentialSmoothing => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Season:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_season).range(1..=365),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                            ui.small("1 = simple smoothing");
                        });
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Forecast periods:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_horizon).range(1..=100),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                    }
                }
                ui.small("The series is read in row order; missing values are dropped.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_timeseries_dialog = open && !run;
        if run {
            self.run_timeseries_dialog();
        }
    }
    /// Opens the time-series diagnostics dialog with the selected series.
    pub fn open_tsdiag_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("The time-series diagnostics need a numeric column");
            return;
        }
        self.tsdiag_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.tsdiag_second = numeric
            .iter()
            .find(|name| **name != self.tsdiag_column)
            .cloned()
            .unwrap_or_default();
        self.show_tsdiag_dialog = true;
    }

    /// Runs the time-series diagnostics of the dialog.
    pub fn run_tsdiag_dialog(&mut self) {
        if self.tsdiag_column.is_empty() {
            self.set_error("Select the series column");
            return;
        }
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let outcome = match self.tsdiag_kind {
            TsDiagKind::Autocorrelation => fp_core::tsdiag::autocorrelation_report(
                &df,
                &self.tsdiag_column,
                self.tsdiag_max_lag,
            ),
            TsDiagKind::CrossCorrelation => {
                if self.tsdiag_second.is_empty() {
                    self.set_error("Select the second series");
                    return;
                }
                fp_core::tsdiag::cross_correlation_report(
                    &df,
                    &self.tsdiag_column,
                    &self.tsdiag_second,
                    self.tsdiag_max_lag,
                )
            }
            TsDiagKind::UnitRoot => fp_core::tsdiag::adf_report(
                &df,
                &self.tsdiag_column,
                self.tsdiag_adf_lags,
                self.tsdiag_adf_model,
            ),
            TsDiagKind::Trend => {
                fp_core::tsdiag::trend_report(&df, &self.tsdiag_column, self.tsdiag_trend)
            }
            TsDiagKind::Difference => {
                match fp_core::tsdiag::add_transformed_column(
                    &mut df,
                    &self.tsdiag_column,
                    self.tsdiag_operation,
                ) {
                    Ok((name, present, missing)) => {
                        self.set_data(df, "Time-series transformation".to_owned());
                        self.append_session(&format!(
                            "Column {name} added: {present} values and {missing} missing.
"
                        ));
                        self.status = format!("Column {name} added");
                        self.show_tsdiag_dialog = false;
                        return;
                    }
                    Err(error) => {
                        self.set_error(error.to_string());
                        return;
                    }
                }
            }
        };
        match outcome {
            Ok(output) => {
                let label = format!("{}: {}", self.tsdiag_kind.name(), self.tsdiag_column);
                self.apply_output(output, label);
                self.show_tsdiag_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Time-series diagnostics dialog.
    pub(super) fn tsdiag_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_tsdiag_dialog {
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
        let labels = [
            (TsDiagKind::Autocorrelation, "Autocorrelation"),
            (TsDiagKind::CrossCorrelation, "Cross-correlation"),
            (TsDiagKind::UnitRoot, "Unit root"),
            (TsDiagKind::Difference, "Differences and lags"),
            (TsDiagKind::Trend, "Trend"),
        ]
        .map(|(kind, text)| (kind, self.catalog().label(text)));
        // Catalogue borrows end before a field is borrowed mutably.
        let order_label = self.catalog().label("Order:");
        let lag_label = self.catalog().label("Lag:");
        let model_label = self.catalog().label("Model:");
        let difference_label = self.catalog().label("Difference");
        let lag_operation_label = self.catalog().label("Lag");
        let adf_labels = [
            (
                fp_core::tsdiag::AdfModel::None,
                self.catalog().label("None"),
            ),
            (
                fp_core::tsdiag::AdfModel::Constant,
                self.catalog().label("Constant"),
            ),
            (
                fp_core::tsdiag::AdfModel::Trend,
                self.catalog().label("Constant and trend"),
            ),
        ];
        let trend_labels = [
            (
                fp_core::tsdiag::TrendModel::Linear,
                self.catalog().label("Linear"),
            ),
            (
                fp_core::tsdiag::TrendModel::Quadratic,
                self.catalog().label("Quadratic"),
            ),
            (
                fp_core::tsdiag::TrendModel::Exponential,
                self.catalog().label("Exponential"),
            ),
            (
                fp_core::tsdiag::TrendModel::Growth,
                self.catalog().label("Growth"),
            ),
        ];
        let mut open = self.show_tsdiag_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("Time Series Diagnostics"))
            .open(&mut open)
            .resizable(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(self.catalog().label("Series:"));
                    egui::ComboBox::from_id_salt("tsdiag_column")
                        .selected_text(if self.tsdiag_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.tsdiag_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.tsdiag_column,
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
                    ui.help_label(self.catalog().label("Command:"));
                    for (kind, text) in labels {
                        ui.selectable_value(&mut self.tsdiag_kind, kind, text);
                    }
                });
                if self.tsdiag_kind == TsDiagKind::CrossCorrelation {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(self.catalog().label("Second series:"));
                        egui::ComboBox::from_id_salt("tsdiag_second")
                            .selected_text(if self.tsdiag_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.tsdiag_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.tsdiag_second,
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
                match self.tsdiag_kind {
                    TsDiagKind::Autocorrelation | TsDiagKind::CrossCorrelation => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label(self.catalog().label("Maximum lag:"));
                            ui.add(egui::DragValue::new(&mut self.tsdiag_max_lag).range(1..=200))
                                .labelled_by(label.id)
                                .hover_from(&label);
                        });
                    }
                    TsDiagKind::UnitRoot => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label(self.catalog().label("Lags:"));
                            ui.add(egui::DragValue::new(&mut self.tsdiag_adf_lags).range(0..=12))
                                .labelled_by(label.id)
                                .hover_from(&label);
                        });
                        ui.horizontal(|ui| {
                            ui.help_label(model_label);
                            for (model, text) in adf_labels {
                                ui.selectable_value(&mut self.tsdiag_adf_model, model, text);
                            }
                        });
                    }
                    TsDiagKind::Difference => {
                        ui.horizontal(|ui| {
                            ui.help_label(self.catalog().label("Operation:"));
                            let difference = matches!(
                                self.tsdiag_operation,
                                fp_core::tsdiag::ColumnOperation::Difference { .. }
                            );
                            if ui.selectable_label(difference, difference_label).clicked() {
                                self.tsdiag_operation =
                                    fp_core::tsdiag::ColumnOperation::Difference {
                                        order: 1,
                                        lag: 1,
                                    };
                            }
                            if ui
                                .selectable_label(!difference, lag_operation_label)
                                .clicked()
                            {
                                self.tsdiag_operation =
                                    fp_core::tsdiag::ColumnOperation::Lag { lag: 1 };
                            }
                        });
                        match &mut self.tsdiag_operation {
                            fp_core::tsdiag::ColumnOperation::Difference { order, lag } => {
                                ui.horizontal(|ui| {
                                    let label = ui.help_label(order_label);
                                    ui.add(egui::DragValue::new(order).range(1..=2))
                                        .labelled_by(label.id)
                                        .hover_from(&label);
                                    let label = ui.help_label(lag_label);
                                    ui.add(egui::DragValue::new(lag).range(1..=50))
                                        .labelled_by(label.id)
                                        .hover_from(&label);
                                });
                            }
                            fp_core::tsdiag::ColumnOperation::Lag { lag } => {
                                ui.horizontal(|ui| {
                                    let label = ui.help_label(lag_label);
                                    ui.add(egui::DragValue::new(lag).range(1..=50))
                                        .labelled_by(label.id)
                                        .hover_from(&label);
                                });
                            }
                        }
                    }
                    TsDiagKind::Trend => {
                        ui.horizontal(|ui| {
                            ui.help_label(model_label);
                            for (model, text) in trend_labels {
                                ui.selectable_value(&mut self.tsdiag_trend, model, text);
                            }
                        });
                    }
                }
                ui.small(match self.tsdiag_kind {
                    TsDiagKind::Autocorrelation => self.catalog().label(
                        "ACF and PACF with the white-noise band and the Ljung–Box statistic.",
                    ),
                    TsDiagKind::CrossCorrelation => self.catalog().label(
                        "Correlation of the first series with the second at lags −m…m; a positive \
                         lag pairs x(t) with y(t+k).",
                    ),
                    TsDiagKind::UnitRoot => self.catalog().label(
                        "Augmented Dickey–Fuller test with the chosen deterministic terms and \
                         augmentation lags.",
                    ),
                    TsDiagKind::Difference => self.catalog().label(
                        "The new column keeps the rows aligned and leaves a missing value where \
                         the observation does not exist.",
                    ),
                    TsDiagKind::Trend => self.catalog().label(
                        "The exponential and growth models need positive values; their \
                         coefficients belong to the linearised fit.",
                    ),
                });
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About time series diagnostics…"))
                        .clicked()
                    {
                        self.show_tsdiag_help = true;
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
        self.show_tsdiag_dialog = open && !run && !cancel;
        if run {
            self.run_tsdiag_dialog();
        }
    }

    /// Help page of the time-series diagnostics.
    pub(super) fn tsdiag_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_tsdiag_help {
            return;
        }
        let mut open = self.show_tsdiag_help;
        let mut close = false;
        dialog_window(self.catalog().label("Time series diagnostics help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The diagnostics read the series in row order. Autocorrelation gives the \
                     ACF, the PACF (Durbin–Levinson) and the Ljung–Box statistic. \
                     Cross-correlation correlates two series at lags −m…m. The unit-root test is \
                     the augmented Dickey–Fuller regression, decided against the asymptotic \
                     critical values. Differences and lags write a new column with the rows \
                     aligned. Trend analysis fits linear, quadratic, exponential or growth \
                     models to the series.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose a numeric series and the command. Autocorrelation and \
                     cross-correlation need the maximum lag (at most n − 2); the unit-root test \
                     takes the augmentation lags and the deterministic terms (none, constant or \
                     constant and trend); differences take the order (1 or 2) and the lag; the \
                     trend models need at least four positive values for the exponential and \
                     growth fits. Missing values are dropped, and a difference or lag keeps a \
                     missing value instead of inventing an observation.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A series that follows another by two observations has its largest \
                     cross-correlation at lag 2. A random walk is not rejected by the unit-root \
                     test, while a mean-reverting series is. Differencing a series once removes \
                     the linear trend, and the exponential trend of a series that doubles every \
                     period has a growth of 100 %.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_tsdiag_help = open && !close;
    }

    /// Opens the ARIMA dialog with the selected series.
    pub fn open_arima_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("ARIMA needs a numeric series column");
            return;
        }
        self.arima_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.show_arima_dialog = true;
    }

    /// Runs the ARIMA model of the dialog.
    pub fn run_arima_dialog(&mut self) {
        if self.arima_column.is_empty() {
            self.set_error("Select the series column");
            return;
        }
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let confidence = (self.arima_confidence / 100.0).clamp(0.5, 0.999);
        let outcome = if self.arima_best {
            fp_core::arima::best(&df, &self.arima_column, self.arima_horizon, confidence)
        } else {
            fp_core::arima::analyze(
                &df,
                &self.arima_column,
                fp_core::arima::ArimaOptions {
                    p: self.arima_p,
                    d: self.arima_d,
                    q: self.arima_q,
                    horizon: self.arima_horizon,
                },
                confidence,
            )
        };
        match outcome {
            Ok(output) => {
                let label = format!("ARIMA: {}", self.arima_column);
                self.apply_output(output, label);
                self.show_arima_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// ARIMA dialog.
    pub(super) fn arima_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_arima_dialog {
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
        let mut open = self.show_arima_dialog;
        let mut run = false;
        let mut cancel = false;
        dialog_window(self.catalog().label("ARIMA"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("arima_column")
                        .selected_text(if self.arima_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.arima_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.arima_column,
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
                    ui.help_checkbox(&mut self.arima_best, "Best order by AICc");
                });
                ui.horizontal(|ui| {
                    let p = ui.help_label("AR:");
                    ui.add_enabled(
                        !self.arima_best,
                        egui::DragValue::new(&mut self.arima_p).range(0..=2),
                    )
                    .labelled_by(p.id)
                    .hover_from(&p);
                    let d = ui.help_label("Difference:");
                    ui.add_enabled(
                        !self.arima_best,
                        egui::DragValue::new(&mut self.arima_d).range(0..=2),
                    )
                    .labelled_by(d.id)
                    .hover_from(&d);
                    let q = ui.help_label("MA:");
                    ui.add_enabled(
                        !self.arima_best,
                        egui::DragValue::new(&mut self.arima_q).range(0..=2),
                    )
                    .labelled_by(q.id)
                    .hover_from(&q);
                });
                ui.horizontal(|ui| {
                    let horizon = ui.help_label("Forecast periods:");
                    ui.add(
                        egui::DragValue::new(&mut self.arima_horizon).range(1..=120),
                    )
                    .labelled_by(horizon.id)
                    .hover_from(&horizon);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.arima_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                ui.small(
                    "Conditional sum of squares; the order search tries p, d and q from 0 to 2                      and keeps the smallest AICc.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About ARIMA…"))
                        .clicked()
                    {
                        self.show_arima_help = true;
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
        self.show_arima_dialog = open && !run && !cancel;
        if run {
            self.run_arima_dialog();
        }
    }

    /// Help page of ARIMA.
    pub(super) fn arima_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_arima_help {
            return;
        }
        let mut open = self.show_arima_help;
        let mut close = false;
        dialog_window(self.catalog().label("ARIMA help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "ARIMA models a series with autoregressive (AR) and moving-average (MA) \
                     terms after differencing it d times. The report gives the coefficients, the \
                     information criteria, the forecasts and their intervals; the best order \
                     searches p, d and q from 0 to 2 and keeps the smallest AICc.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose one numeric column; non-finite values are excluded. Set the AR, \
                     differencing and MA orders, or let the best-order search choose them. The \
                     forecast periods are between 1 and 120. The estimation is conditional and \
                     the intervals ignore parameter uncertainty.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A series with y(t) = 0.6·y(t−1) + noise is fitted by ARIMA(1,0,0); the AR \
                     coefficient is near 0.6 and the forecasts decay towards the mean. A random \
                     walk needs one difference: the forecast starts at the last value and the \
                     interval widens with the horizon.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_arima_help = open && !close;
    }

    pub fn open_series_dialog(&mut self) {
        self.series_column = self.selected.clone().unwrap_or_default();
        self.series_time.clear();
        self.show_series_dialog = true;
    }
    pub fn run_series_dialog(&mut self) {
        if self.series_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let time =
            (!self.series_time.trim().is_empty()).then(|| self.series_time.trim().to_owned());
        if time.as_deref() == Some(self.series_column.as_str()) {
            self.set_error("The time column must be different from the series");
            return;
        }
        let spec = AnalysisSpec::series_plot(self.series_column.clone()).with_series_time(time);
        self.execute_spec(spec, format!("Time Series Plot: {}", self.series_column));
        self.show_series_dialog = false;
    }
    pub(super) fn series_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_series_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_series_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Time Series Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("series_column")
                        .selected_text(if self.series_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.series_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.series_column,
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
                    let label = ui.help_label("Time (optional):");
                    egui::ComboBox::from_id_salt("series_time")
                        .selected_text(if self.series_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.series_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.series_time, String::new(), "—");
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.series_time,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Without a time column the observations are plotted in row order.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_series_dialog = open && !run;
        if run {
            self.run_series_dialog();
        }
    }
    pub fn open_stem_dialog(&mut self) {
        self.stem_column = self.selected.clone().unwrap_or_default();
        self.show_stem_dialog = true;
    }
    pub fn run_stem_dialog(&mut self) {
        if self.stem_column.is_empty() {
            self.set_error("Select the column to display");
            return;
        }
        let spec = AnalysisSpec::stem_leaf(self.stem_column.clone());
        self.execute_spec(spec, format!("Stem-and-Leaf: {}", self.stem_column));
        self.show_stem_dialog = false;
    }
    pub(super) fn stem_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_stem_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_stem_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stem-and-Leaf"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("stem_column")
                        .selected_text(if self.stem_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.stem_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.stem_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The leaf unit is chosen to give between five and twenty stems.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_stem_dialog = open && !run;
        if run {
            self.run_stem_dialog();
        }
    }
    pub fn open_ecdf_dialog(&mut self) {
        self.ecdf_column = self.selected.clone().unwrap_or_default();
        self.show_ecdf_dialog = true;
    }
    pub fn run_ecdf_dialog(&mut self) {
        if self.ecdf_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let spec = AnalysisSpec::ecdf(self.ecdf_column.clone())
            .with_ecdf_confidence((self.ecdf_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Empirical CDF: {}", self.ecdf_column));
        self.show_ecdf_dialog = false;
    }
    pub(super) fn ecdf_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_ecdf_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_ecdf_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Empirical CDF"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("ecdf_column")
                        .selected_text(if self.ecdf_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.ecdf_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.ecdf_column,
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
                    let label = ui.help_label("Band confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.ecdf_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Step function with a Dvoretzky–Kiefer–Wolfowitz band.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_ecdf_dialog = open && !run;
        if run {
            self.run_ecdf_dialog();
        }
    }
    pub fn open_dotplot_dialog(&mut self) {
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
        self.dot_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.dot_factor.clear();
        self.show_dot_dialog = true;
    }
    pub fn run_dotplot_dialog(&mut self) {
        if self.dot_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let factor =
            (!self.dot_factor.trim().is_empty()).then(|| self.dot_factor.trim().to_owned());
        if factor.as_deref() == Some(self.dot_column.as_str()) {
            self.set_error("The factor must be a different column");
            return;
        }
        let spec = AnalysisSpec::dotplot(self.dot_column.clone()).with_dotplot_factor(factor);
        let label = format!("Dotplot: {}", self.dot_column);
        self.execute_spec(spec, label);
        self.show_dot_dialog = false;
    }
    /// Opens the probability distribution plot dialog.
    pub fn open_pdp_dialog(&mut self) {
        self.pdp_parameters = fp_core::pdp::default_parameters(self.pdp_kind)
            .into_iter()
            .chain(std::iter::repeat(1.0))
            .take(3)
            .collect();
        self.pdp_shade_low.clear();
        self.pdp_shade_high.clear();
        self.show_pdp_dialog = true;
    }

    /// Builds the probability distribution plot of the dialog.
    pub fn run_pdp_dialog(&mut self) {
        let count = fp_core::pdp::parameter_count(self.pdp_kind);
        let parameters: Vec<f64> = self.pdp_parameters.iter().copied().take(count).collect();
        let number = |text: &str| {
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
        };
        let shade = match (self.pdp_shade_low.trim(), self.pdp_shade_high.trim()) {
            ("", "") => None,
            (low, high) => {
                let (Some(low), Some(high)) = (number(low), number(high)) else {
                    self.set_error("The shaded bounds must be numbers or both blank");
                    return;
                };
                if (low - high).abs() <= f64::EPSILON {
                    self.set_error("The shaded bounds must differ");
                    return;
                }
                Some((low, high))
            }
        };
        let options = fp_core::pdp::PdpOptions {
            kind: self.pdp_kind,
            parameters,
            view: self.pdp_view,
            shade,
        };
        match fp_core::pdp::build(&options) {
            Ok(output) => {
                let label = format!("Probability distribution plot: {}", self.pdp_kind.name());
                self.apply_output(output, label);
                self.show_pdp_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Probability distribution plot dialog.
    pub(super) fn pdp_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pdp_dialog {
            return;
        }
        let mut open = self.show_pdp_dialog;
        let mut run = false;
        let mut cancel = false;
        let mut reset = false;
        dialog_window(self.catalog().label("Probability Distribution Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Distribution:");
                    egui::ComboBox::from_id_salt("pdp_kind")
                        .selected_text(self.pdp_kind.name())
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            for kind in [
                                fp_core::workflow::DistributionKind::Normal,
                                fp_core::workflow::DistributionKind::Lognormal,
                                fp_core::workflow::DistributionKind::Weibull,
                                fp_core::workflow::DistributionKind::Exponential,
                                fp_core::workflow::DistributionKind::Logistic,
                                fp_core::workflow::DistributionKind::Loglogistic,
                                fp_core::workflow::DistributionKind::Gamma,
                                fp_core::workflow::DistributionKind::ExtremeValue,
                                fp_core::workflow::DistributionKind::Weibull3,
                                fp_core::workflow::DistributionKind::Lognormal3,
                            ] {
                                if ui
                                    .selectable_value(&mut self.pdp_kind, kind, kind.name())
                                    .changed()
                                {
                                    reset = true;
                                }
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if reset {
                    let defaults = fp_core::pdp::default_parameters(self.pdp_kind);
                    self.pdp_parameters = defaults
                        .into_iter()
                        .chain(std::iter::repeat(1.0))
                        .take(3)
                        .collect();
                }
                let labels = fp_core::pdp::parameter_labels(self.pdp_kind);
                for (index, label_text) in labels.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(*label_text);
                        ui.add(egui::DragValue::new(&mut self.pdp_parameters[index]).speed(0.1))
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("View:");
                    egui::ComboBox::from_id_salt("pdp_view")
                        .selected_text(self.pdp_view.name())
                        .width(180.0)
                        .show_ui(ui, |ui| {
                            for view in [
                                fp_core::pdp::PdpView::Density,
                                fp_core::pdp::PdpView::Cumulative,
                            ] {
                                ui.selectable_value(&mut self.pdp_view, view, view.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                for (label_text, field) in [
                    ("Shade from:", &mut self.pdp_shade_low),
                    ("Shade to:", &mut self.pdp_shade_high),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(label_text);
                        ui.text_edit_singleline(field)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small(
                    "The plot is theoretical: no worksheet column is used. Leave the shaded \
                     bounds blank for the plain curve.",
                );
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About this plot…"))
                        .clicked()
                    {
                        self.show_pdp_help = true;
                    }
                    ui.small(self.catalog().label("Overview, data and an example"));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("OK")).clicked() {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_pdp_dialog = open && !run && !cancel;
        if run {
            self.run_pdp_dialog();
        }
    }

    /// Help page of the probability distribution plot.
    pub(super) fn pdp_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_pdp_help {
            return;
        }
        let mut open = self.show_pdp_help;
        let mut close = false;
        dialog_window(self.catalog().label("Probability Distribution Plot help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The probability distribution plot draws the theoretical density or \
                     cumulative probability of a distribution you specify. No worksheet data are \
                     used. You can shade the area between two values to read the probability.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Data"));
                ui.label(self.catalog().label(
                    "Choose one of the ten distributions and enter its parameters; the labels \
                     change with the family. Choose Density or Cumulative probability. \
                     Optionally enter two values of x to shade the area between them; the report \
                     gives the shaded probability F(b) − F(a).",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "A standard normal (mean 0, standard deviation 1) peaks at 0.399. Shading \
                     from −1 to 1 covers 68.27 % of the area; the cumulative curve reaches 0.5 \
                     at the mean.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_pdp_help = open && !close;
    }

    pub(super) fn dotplot_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_dot_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_dot_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Dotplot"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("dot_column")
                        .selected_text(if self.dot_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.dot_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.dot_column,
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
                    let label = ui.help_label("By (optional):");
                    egui::ComboBox::from_id_salt("dot_factor")
                        .selected_text(if self.dot_factor.is_empty() {
                            "—".to_owned()
                        } else {
                            self.dot_factor.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.dot_factor, String::new(), "—");
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.dot_factor,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("One dot per observation; coincident values are stacked.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_dot_dialog = open && !run;
        if run {
            self.run_dotplot_dialog();
        }
    }
    pub fn open_calc_dialog(&mut self) {
        self.calc_name = format!(
            "C{}",
            self.data.as_ref().map_or(1, DataFrame::n_columns) + 1
        );
        self.calc_expression.clear();
        self.show_calc_dialog = true;
    }
    pub fn run_calc_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.calc_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        if self.calc_expression.trim().is_empty() {
            self.set_error("Write the expression");
            return;
        }
        match fp_core::calc::add_column(&mut df, self.calc_name.trim(), &self.calc_expression) {
            Ok(()) => {
                let name = self.calc_name.trim().to_owned();
                self.set_data(df, "Calculated column".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} calculated.\n",
                    self.calc_expression.trim()
                ));
                self.status = format!("Column {name} added");
                self.show_calc_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }
    pub(super) fn calc_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_calc_dialog {
            return;
        }
        let mut open = self.show_calc_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Calculator"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.add(egui::TextEdit::singleline(&mut self.calc_name).desired_width(140.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Expression:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.calc_expression)
                            .desired_width(250.0)
                            .hint_text("[weight] / ([height] ^ 2)"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Columns in brackets or by name; + - * / ^, parentheses, sqrt, ln, log10, \
                     exp, abs, round, floor, ceil, min, max, pow.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_calc_dialog = open && !run;
        if run {
            self.run_calc_dialog();
        }
    }
    /// Opens the data and calculations dialog.
    pub fn open_data_calc_dialog(&mut self) {
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
            self.set_error("The worksheet has no numeric column");
            return;
        }
        self.data_columns = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .map(|name| vec![name])
            .unwrap_or_else(|| numeric.iter().take(1).cloned().collect());
        self.data_second_columns.clear();
        self.data_first_column = numeric[0].clone();
        self.data_second_column = numeric.get(1).cloned().unwrap_or_default();
        self.data_name = "Pattern".to_owned();
        for index in 0..3 {
            self.data_mesh_names[index] = format!("F{}", index + 1);
            self.data_mesh_from[index] = "0".to_owned();
            self.data_mesh_to[index] = "1".to_owned();
            self.data_mesh_levels[index] = 3;
        }
        self.show_data_calc_dialog = true;
    }

    fn data_number(&self, text: &str, label: &str) -> Result<f64, String> {
        text.trim()
            .parse::<f64>()
            .map_err(|_| format!("{label} must be a number"))
    }

    /// Runs the task of the data and calculations dialog.
    pub fn run_data_calc_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let columns = self.data_columns.clone();
        let done = |app: &mut Self, message: String| {
            app.status = message;
            app.show_data_calc_dialog = false;
        };
        match self.data_task {
            DataTask::Standardize => match fp_core::calcdata::standardize(&df, &columns) {
                Ok(out) => {
                    let count = columns.len();
                    self.set_data(out, "Standardized data".to_owned());
                    done(self, format!("{count} column(s) standardised"));
                }
                Err(error) => self.set_error(error.to_string()),
            },
            DataTask::ColumnStatistics => {
                match fp_core::calcdata::column_statistics_report(&df, &columns) {
                    Ok(output) => {
                        self.apply_output(output, "Column statistics".to_owned());
                        self.show_data_calc_dialog = false;
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::RowStatistics => {
                if self.data_name.trim().is_empty() {
                    self.set_error("Name the new column");
                    return;
                }
                let mut out = df.clone();
                let name = self.data_name.trim().to_owned();
                match fp_core::calcdata::add_row_statistics(
                    &mut out,
                    &name,
                    &columns,
                    self.data_row_statistic,
                ) {
                    Ok(()) => {
                        self.set_data(out, "Row statistics".to_owned());
                        done(self, format!("Column {name} added"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Patterned => {
                let start = match self.data_number(&self.data_start, "The start") {
                    Ok(value) => value,
                    Err(message) => {
                        self.set_error(message);
                        return;
                    }
                };
                let step = match self.data_number(&self.data_step, "The step") {
                    Ok(value) => value,
                    Err(message) => {
                        self.set_error(message);
                        return;
                    }
                };
                match fp_core::calcdata::patterned_data(
                    &self.data_name,
                    start,
                    step,
                    self.data_length,
                    self.data_repeats,
                ) {
                    Ok(out) => {
                        let rows = out.n_rows();
                        self.set_data(out, "Patterned data".to_owned());
                        done(self, format!("Pattern with {rows} row(s)"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Mesh => {
                let count = self.data_mesh_count.clamp(1, 3);
                let mut factors = Vec::with_capacity(count);
                for index in 0..count {
                    let name = self.data_mesh_names[index].trim().to_owned();
                    if name.is_empty() {
                        self.set_error("Name every factor");
                        return;
                    }
                    let from = match self
                        .data_number(&self.data_mesh_from[index].clone(), "The first level")
                    {
                        Ok(value) => value,
                        Err(message) => {
                            self.set_error(message);
                            return;
                        }
                    };
                    let to = match self.data_number(&self.data_mesh_to[index], "The last level") {
                        Ok(value) => value,
                        Err(message) => {
                            self.set_error(message);
                            return;
                        }
                    };
                    factors.push(fp_core::calcdata::MeshFactor {
                        name,
                        from,
                        to,
                        levels: self.data_mesh_levels[index],
                    });
                }
                match fp_core::calcdata::mesh_data(&factors) {
                    Ok(out) => {
                        let rows = out.n_rows();
                        self.set_data(out, "Mesh data".to_owned());
                        done(self, format!("Mesh with {rows} row(s)"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Indicators => {
                if self.data_first_column.is_empty() {
                    self.set_error("Select the factor column");
                    return;
                }
                match fp_core::calcdata::indicator_variables(
                    &df,
                    &self.data_first_column,
                    self.data_drop_first,
                ) {
                    Ok(out) => {
                        let added = out.n_columns() - df.n_columns();
                        self.set_data(out, "Indicator variables".to_owned());
                        done(self, format!("{added} indicator column(s) added"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Transpose => match fp_core::calcdata::transpose(&df, &columns) {
                Ok(out) => {
                    let rows = out.n_rows();
                    let width = out.n_columns();
                    self.set_data(out, "Transposed block".to_owned());
                    done(self, format!("Transposed block: {rows} × {width}"));
                }
                Err(error) => self.set_error(error.to_string()),
            },
            DataTask::Product => {
                if self.data_second_columns.is_empty() {
                    self.set_error("Select the columns of the second block");
                    return;
                }
                match fp_core::calcdata::matrix_product(
                    &df,
                    &columns,
                    &df,
                    &self.data_second_columns,
                ) {
                    Ok(out) => {
                        let rows = out.n_rows();
                        let width = out.n_columns();
                        self.set_data(out, "Matrix product".to_owned());
                        done(self, format!("Product: {rows} × {width}"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Random => {
                let first = match self.data_number(&self.data_first, "The first parameter") {
                    Ok(value) => value,
                    Err(message) => {
                        self.set_error(message);
                        return;
                    }
                };
                let second =
                    match self.data_number(&self.data_second.clone(), "The second parameter") {
                        Ok(value) => value,
                        Err(message) => {
                            self.set_error(message);
                            return;
                        }
                    };
                match fp_core::calcdata::random_data(
                    &self.data_name,
                    self.data_distribution,
                    self.data_count,
                    first,
                    second,
                    self.data_seed,
                ) {
                    Ok(out) => {
                        let rows = out.n_rows();
                        self.set_data(out, "Random data".to_owned());
                        done(self, format!("{rows} random value(s)"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Bootstrap => {
                match fp_core::calcdata::bootstrap_intervals(
                    &df,
                    &columns,
                    self.data_reps,
                    self.data_seed,
                    0.95,
                ) {
                    Ok((out, intervals)) => {
                        let rows = out.n_rows();
                        self.set_data(out, "Bootstrap samples".to_owned());
                        let mut note =
                            String::from("Bootstrap percentile intervals (95 % of the mean):\n");
                        for interval in &intervals {
                            note.push_str(&format!(
                                "  {} mean {:.6}, interval ({:.6}, {:.6})\n",
                                interval.column, interval.mean, interval.lower, interval.upper
                            ));
                        }
                        self.append_session(&note);
                        done(self, format!("Bootstrap table with {rows} row(s)"));
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
            DataTask::Randomization => {
                let first = self.data_first_column.clone();
                let second = self.data_second_column.clone();
                match fp_core::calcdata::randomization_analysis(
                    &df,
                    &first,
                    &second,
                    self.data_reps,
                    self.data_seed,
                ) {
                    Ok(output) => {
                        self.apply_output(output, "Randomization test".to_owned());
                        self.show_data_calc_dialog = false;
                    }
                    Err(error) => self.set_error(error.to_string()),
                }
            }
        }
    }

    fn data_column_selector(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        numeric: &[String],
        second: bool,
    ) {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .max_height(130.0)
            .show(ui, |ui| {
                for name in numeric {
                    let current = if second {
                        self.data_second_columns.contains(name)
                    } else {
                        self.data_columns.contains(name)
                    };
                    let mut checked = current;
                    if ui.checkbox(&mut checked, name).changed() {
                        let list = if second {
                            &mut self.data_second_columns
                        } else {
                            &mut self.data_columns
                        };
                        if checked {
                            list.push(name.clone());
                        } else {
                            list.retain(|value| value != name);
                        }
                    }
                }
            });
    }

    /// Data and calculations dialog: the task, its inputs and the buttons.
    pub(super) fn data_calc_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_data_calc_dialog {
            return;
        }
        let (numeric, all): (Vec<String>, Vec<String>) = self
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
        let mut open = self.show_data_calc_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Data and Calculations"))
            .open(&mut open)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(self.catalog().label("Task:"));
                    egui::ComboBox::from_id_salt("data_task")
                        .selected_text(self.data_task.name())
                        .show_ui(ui, |ui| {
                            for task in [
                                DataTask::Standardize,
                                DataTask::ColumnStatistics,
                                DataTask::RowStatistics,
                                DataTask::Patterned,
                                DataTask::Mesh,
                                DataTask::Indicators,
                                DataTask::Transpose,
                                DataTask::Product,
                                DataTask::Random,
                                DataTask::Bootstrap,
                                DataTask::Randomization,
                            ] {
                                ui.selectable_value(&mut self.data_task, task, task.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| match self.data_task {
                        DataTask::Standardize => {
                            ui.small(self.catalog().label(
                                "Appends <column>_z, centred on the mean and divided by the \
                                 sample standard deviation.",
                            ));
                            ui.label("Columns:");
                            self.data_column_selector(ui, "data_columns", &numeric, false);
                        }
                        DataTask::ColumnStatistics => {
                            ui.label("Columns:");
                            self.data_column_selector(ui, "data_stat_columns", &numeric, false);
                        }
                        DataTask::RowStatistics => {
                            ui.label(self.catalog().label("Columns:"));
                            self.data_column_selector(ui, "data_row_columns", &numeric, false);
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Statistic:"));
                                egui::ComboBox::from_id_salt("data_row_statistic")
                                    .selected_text(self.data_row_statistic.name())
                                    .show_ui(ui, |ui| {
                                        for statistic in [
                                            fp_core::calcdata::RowStatistic::Sum,
                                            fp_core::calcdata::RowStatistic::Mean,
                                            fp_core::calcdata::RowStatistic::Minimum,
                                            fp_core::calcdata::RowStatistic::Maximum,
                                            fp_core::calcdata::RowStatistic::Range,
                                            fp_core::calcdata::RowStatistic::StandardDeviation,
                                        ] {
                                            ui.selectable_value(
                                                &mut self.data_row_statistic,
                                                statistic,
                                                statistic.name(),
                                            );
                                        }
                                    })
                                    .response
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("New column:"));
                                ui.text_edit_singleline(&mut self.data_name)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                        }
                        DataTask::Patterned => {
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Column:"));
                                ui.text_edit_singleline(&mut self.data_name)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Start:"));
                                ui.text_edit_singleline(&mut self.data_start)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                let label = ui.help_label(self.catalog().label("Step:"));
                                ui.text_edit_singleline(&mut self.data_step)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Length:"));
                                ui.add(
                                    egui::DragValue::new(&mut self.data_length).range(1..=10000),
                                )
                                .labelled_by(label.id)
                                .hover_from(&label);
                                let label = ui.help_label(self.catalog().label("Repeats:"));
                                ui.add(
                                    egui::DragValue::new(&mut self.data_repeats).range(1..=1000),
                                )
                                .labelled_by(label.id)
                                .hover_from(&label);
                            });
                        }
                        DataTask::Mesh => {
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Factors:"));
                                ui.add(
                                    egui::DragValue::new(&mut self.data_mesh_count).range(1..=3),
                                )
                                .labelled_by(label.id)
                                .hover_from(&label);
                            });
                            let count = self.data_mesh_count.clamp(1, 3);
                            for index in 0..count {
                                let name_label = self.catalog().label(match index {
                                    0 => "Name 1:",
                                    1 => "Name 2:",
                                    _ => "Name 3:",
                                });
                                ui.horizontal(|ui| {
                                    let label = ui.help_label(name_label);
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.data_mesh_names[index],
                                        )
                                        .desired_width(110.0),
                                    )
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                });
                                let first_label = self.catalog().label(match index {
                                    0 => "First 1:",
                                    1 => "First 2:",
                                    _ => "First 3:",
                                });
                                let last_label = self.catalog().label(match index {
                                    0 => "Last 1:",
                                    1 => "Last 2:",
                                    _ => "Last 3:",
                                });
                                let levels_label = self.catalog().label(match index {
                                    0 => "Levels 1:",
                                    1 => "Levels 2:",
                                    _ => "Levels 3:",
                                });
                                ui.horizontal(|ui| {
                                    let label = ui.help_label(first_label);
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.data_mesh_from[index])
                                            .desired_width(50.0),
                                    )
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                    let label = ui.help_label(last_label);
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.data_mesh_to[index])
                                            .desired_width(50.0),
                                    )
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                    let label = ui.help_label(levels_label);
                                    ui.add(
                                        egui::DragValue::new(&mut self.data_mesh_levels[index])
                                            .range(2..=100),
                                    )
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                });
                            }
                            ui.small(self.catalog().label(
                                "Each factor is a name, the first and last level and the number \
                                 of equally spaced levels; the first factor varies fastest.",
                            ));
                        }
                        DataTask::Indicators => {
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Factor column:"));
                                egui::ComboBox::from_id_salt("data_factor")
                                    .selected_text(if self.data_first_column.is_empty() {
                                        "—".to_owned()
                                    } else {
                                        self.data_first_column.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in &all {
                                            ui.selectable_value(
                                                &mut self.data_first_column,
                                                name.clone(),
                                                name.as_str(),
                                            );
                                        }
                                    })
                                    .response
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            let drop_first =
                                self.catalog().label("Drop the first level (reference)");
                            ui.checkbox(&mut self.data_drop_first, drop_first);
                        }
                        DataTask::Transpose => {
                            ui.small(
                                self.catalog().label(
                                    "Each selected column becomes a row; each row a column.",
                                ),
                            );
                            ui.label("Columns:");
                            self.data_column_selector(ui, "data_transpose", &numeric, false);
                        }
                        DataTask::Product => {
                            ui.small(self.catalog().label(
                                "Aᵀ·B: both blocks have the same rows; the result has one row per \
                                 column of the first block and one column per column of the \
                                 second.",
                            ));
                            ui.label("First block:");
                            self.data_column_selector(ui, "data_product_a", &numeric, false);
                            ui.label("Second block:");
                            self.data_column_selector(ui, "data_product_b", &numeric, true);
                        }
                        DataTask::Random => {
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Column:"));
                                ui.text_edit_singleline(&mut self.data_name)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Distribution:"));
                                egui::ComboBox::from_id_salt("data_distribution")
                                    .selected_text(self.data_distribution.name())
                                    .show_ui(ui, |ui| {
                                        for distribution in [
                                            fp_core::calcdata::RandomDistribution::Normal,
                                            fp_core::calcdata::RandomDistribution::Uniform,
                                            fp_core::calcdata::RandomDistribution::Exponential,
                                        ] {
                                            ui.selectable_value(
                                                &mut self.data_distribution,
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
                                let first_label = match self.data_distribution {
                                    fp_core::calcdata::RandomDistribution::Normal => "Mean:",
                                    fp_core::calcdata::RandomDistribution::Uniform => "Minimum:",
                                    fp_core::calcdata::RandomDistribution::Exponential => "Rate:",
                                };
                                let label = ui.help_label(self.catalog().label(first_label));
                                ui.text_edit_singleline(&mut self.data_first)
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                if self.data_distribution
                                    != fp_core::calcdata::RandomDistribution::Exponential
                                {
                                    let second_label = match self.data_distribution {
                                        fp_core::calcdata::RandomDistribution::Normal => "Std dev:",
                                        _ => "Maximum:",
                                    };
                                    let label = ui.help_label(self.catalog().label(second_label));
                                    ui.text_edit_singleline(&mut self.data_second)
                                        .labelled_by(label.id)
                                        .hover_from(&label);
                                }
                            });
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Values:"));
                                ui.add(
                                    egui::DragValue::new(&mut self.data_count).range(1..=100000),
                                )
                                .labelled_by(label.id)
                                .hover_from(&label);
                                let label = ui.help_label(self.catalog().label("Seed:"));
                                ui.add(egui::DragValue::new(&mut self.data_seed).speed(1.0))
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                        }
                        DataTask::Bootstrap => {
                            ui.label("Columns:");
                            self.data_column_selector(ui, "data_bootstrap", &numeric, false);
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("Resamples:"));
                                ui.add(egui::DragValue::new(&mut self.data_reps).range(1..=1000))
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                                let label = ui.help_label(self.catalog().label("Seed:"));
                                ui.add(egui::DragValue::new(&mut self.data_seed).speed(1.0))
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                            ui.small(self.catalog().label(
                                "Each resample copies the whole rows with replacement; the table \
                                 has a Sample column and every selected column, and the Session \
                                 receives the percentile interval of the mean.",
                            ));
                        }
                        DataTask::Randomization => {
                            ui.horizontal(|ui| {
                                let label = ui.help_label(self.catalog().label("First sample:"));
                                egui::ComboBox::from_id_salt("data_first_sample")
                                    .selected_text(if self.data_first_column.is_empty() {
                                        "—".to_owned()
                                    } else {
                                        self.data_first_column.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in &numeric {
                                            ui.selectable_value(
                                                &mut self.data_first_column,
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
                                let label = ui.help_label(self.catalog().label("Second sample:"));
                                egui::ComboBox::from_id_salt("data_second_sample")
                                    .selected_text(if self.data_second_column.is_empty() {
                                        "—".to_owned()
                                    } else {
                                        self.data_second_column.clone()
                                    })
                                    .show_ui(ui, |ui| {
                                        for name in &numeric {
                                            ui.selectable_value(
                                                &mut self.data_second_column,
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
                                let label = ui.help_label(self.catalog().label("Permutations:"));
                                ui.add(
                                    egui::DragValue::new(&mut self.data_reps).range(100..=100000),
                                )
                                .labelled_by(label.id)
                                .hover_from(&label);
                                let label = ui.help_label(self.catalog().label("Seed:"));
                                ui.add(egui::DragValue::new(&mut self.data_seed).speed(1.0))
                                    .labelled_by(label.id)
                                    .hover_from(&label);
                            });
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("About data and calculations…"))
                        .clicked()
                    {
                        self.show_data_calc_help = true;
                    }
                    ui.small(self.catalog().label("Tasks, conventions and an example"));
                });
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Run")).clicked() {
                        run = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_data_calc_dialog = false;
                    }
                });
            });
        self.show_data_calc_dialog = open && !run && self.show_data_calc_dialog;
        if run {
            self.run_data_calc_dialog();
        }
    }

    /// Help page of the data and calculations.
    pub(super) fn data_calc_help_window(&mut self, ctx: &egui::Context) {
        if !self.show_data_calc_help {
            return;
        }
        let mut open = self.show_data_calc_help;
        let mut close = false;
        dialog_window(self.catalog().label("Data and calculations help"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(570.0)
            .show(ctx, |ui| {
                ui.strong(self.catalog().label("Overview"));
                ui.label(self.catalog().label(
                    "The dialog works on the active worksheet. Standardize, the indicator \
                     columns and the row statistics add columns to it (in a new sheet), while \
                     the patterned, mesh, random, bootstrap, transposed and matrix results \
                     open a new sheet of their own. The column statistics and the \
                     randomization test report to the Session window.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Tasks"));
                ui.label(self.catalog().label(
                    "Standardize appends z = (x − mean)/s with the sample standard deviation. \
                     Column statistics give N, N*, sum, mean, standard error, standard \
                     deviation, quartiles and extremes. Row statistics combine the selected \
                     columns row by row, skipping missing cells. Patterned data repeat an \
                     arithmetic sequence. Mesh data build the full grid of one to three \
                     numeric factors. Indicators code a categorical column as 0/1 columns, \
                     with or without the first level. Transpose swaps rows and columns, and \
                     the matrix product multiplies the transpose of the first block by the \
                     second. Random data draw Normal, Uniform or Exponential values. The \
                     bootstrap resamples whole rows with replacement, and the randomization \
                     test permutes the group labels of two columns.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Conventions"));
                ui.label(self.catalog().label(
                    "Every random task takes a seed and uses SplitMix64, so the same seed gives \
                     the same sheet and the same p-value. The randomization p-value is \
                     (extreme + 1)/(permutations + 1) and the test is two-sided on the \
                     difference in means. A missing or non-finite cell is left out of the \
                     statistics and stays missing when a column is standardized.",
                ));
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Example"));
                ui.label(self.catalog().label(
                    "Standardising a column from 1 to 5 gives z from about −1.265 to 1.265, with \
                     mean zero and standard deviation one. A mesh of two factors with three and \
                     two levels has six rows. Indicators of three levels with the reference \
                     dropped add two columns. A bootstrap of five rows and three resamples has \
                     fifteen rows, and two clearly separated samples give a randomization \
                     p-value near zero.",
                ));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Close")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_data_calc_help = open && !close;
    }
}
