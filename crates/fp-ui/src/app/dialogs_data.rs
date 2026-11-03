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
        self.ecdf_confidence = 95.0;
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
}
