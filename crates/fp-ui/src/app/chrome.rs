// SPDX-License-Identifier: MIT
//! Window chrome: the frame, toolbar, status bar and the worksheet tabs.

use super::*;

impl FreePlotsApp {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_analysis(ui.ctx());
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new("fp_dialog_open"), false));
        // Do not zoom past the point where the minimum layout no longer fits
        // the window: on a small screen 200 % would clip the menus and dialogs
        // with no way to scroll to them.
        let max_zoom = max_zoom_for(ui.ctx());
        self.zoom = self.zoom.clamp(0.5, max_zoom);
        // A window close with unsaved work is cancelled once and turned into a
        // prompt. A project save in flight keeps cancelling the close until the
        // runner writes the file.
        if ui.ctx().input(|input| input.viewport().close_requested()) && self.request_close() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.close_after_save && !self.dirty {
            self.close_after_save = false;
            self.close_confirmed = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.close_prompt_window(ui.ctx());
        self.restore_window(ui.ctx());
        if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close_dialogs();
        }
        let (zoom_in, zoom_out, zoom_reset) = ui.ctx().input(|input| {
            (
                input.modifiers.command
                    && (input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)),
                input.modifiers.command && input.key_pressed(egui::Key::Minus),
                input.modifiers.command && input.key_pressed(egui::Key::Num0),
            )
        });
        if zoom_in {
            self.zoom = (self.zoom * 1.1).min(max_zoom);
        }
        if zoom_out {
            self.zoom = (self.zoom / 1.1).max(0.5);
        }
        if zoom_reset {
            self.zoom = 1.0;
        }
        let (new, open, save, print, select_all, find) = ui.ctx().input(|input| {
            let command = input.modifiers.command;
            (
                command && input.key_pressed(egui::Key::N),
                command && input.key_pressed(egui::Key::O),
                command && input.key_pressed(egui::Key::S),
                command && input.key_pressed(egui::Key::P),
                command && input.key_pressed(egui::Key::A),
                command && input.key_pressed(egui::Key::F),
            )
        });
        if !ui.ctx().egui_wants_keyboard_input() {
            if new {
                self.new_worksheet();
            }
            if open {
                self.begin_open(FileKind::Csv);
            }
            if save {
                self.begin_save(FileKind::Csv);
            }
            if print && let Some(directory) = self.storage.temp_dir() {
                self.print_request = Some(directory.join("FreePlots-session.pdf"));
            }
            if select_all {
                self.select_all_cells();
            }
            if find {
                self.open_find_dialog();
            }
        }
        ui.ctx().set_zoom_factor(self.zoom);
        if ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("fp_clear_dialog")))
            .unwrap_or(false)
        {
            self.reset_dialog_defaults();
        }
        let want_light = !self.dark_mode;
        if self.light_visuals != Some(want_light) {
            apply_theme(ui.ctx(), want_light);
            self.light_visuals = Some(want_light);
        }

        egui::Panel::top("fp_menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("fp_toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("fp_status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("fp_sheets").show(ui, |ui| self.sheet_tabs(ui));
        if self.show_session {
            egui::Panel::right("fp_session")
                .resizable(true)
                .default_size(520.0)
                .size_range(320.0..=720.0)
                .show(ui, |ui| self.session_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(if self.dark_mode {
                        Color32::from_rgb(20, 23, 27)
                    } else {
                        Color32::from_rgb(252, 250, 246)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if self.dark_mode {
                            Color32::from_rgb(44, 52, 61)
                        } else {
                            Color32::from_rgb(218, 212, 201)
                        },
                    ))
                    .inner_margin(egui::Margin::same(2)),
            )
            .show(ui, |ui| self.worksheet(ui));

        let ctx = ui.ctx().clone();
        self.open_window(&ctx);
        self.csv_options_dialog(&ctx);
        self.save_window(&ctx);
        self.overwrite_window(&ctx);
        self.rename_window(&ctx);
        self.sheet_rename_window(&ctx);
        self.probability_plot_dialog(&ctx);
        self.capability_dialog(&ctx);
        self.capability_help_window(&ctx);
        self.sixpack_help_window(&ctx);
        self.control_chart_dialog(&ctx);
        self.chart_help_window(&ctx);
        self.cusum_help_window(&ctx);
        self.attribute_chart_dialog(&ctx);
        self.attrcap_dialog(&ctx);
        self.attrcap_help_window(&ctx);
        self.t_test_dialog(&ctx);
        self.equivalence_dialog(&ctx);
        self.rank_test_dialog(&ctx);
        self.runs_dialog(&ctx);
        self.runs_help_window(&ctx);
        self.correlation_dialog(&ctx);
        self.curve_dialog(&ctx);
        self.two_way_dialog(&ctx);
        self.crosstab_dialog(&ctx);
        self.g_chart_dialog(&ctx);
        self.t_chart_dialog(&ctx);
        self.t_chart_help_window(&ctx);
        self.partial_dialog(&ctx);
        self.box_cox_dialog(&ctx);
        self.johnson_dialog(&ctx);
        self.johnson_help_window(&ctx);
        self.tolerance_dialog(&ctx);
        self.linearity_dialog(&ctx);
        self.logistic_dialog(&ctx);
        self.poisson_dialog(&ctx);
        self.ordinal_dialog(&ctx);
        self.pareto_dialog(&ctx);
        self.calc_dialog(&ctx);
        self.sheet_dialog(&ctx);
        self.interval_dialog(&ctx);
        self.matrix_dialog(&ctx);
        self.dotplot_dialog(&ctx);
        self.stem_dialog(&ctx);
        self.series_dialog(&ctx);
        self.ecdf_dialog(&ctx);
        self.alt_dialog(&ctx);
        self.step_stress_dialog(&ctx);
        self.timeseries_dialog(&ctx);
        self.pca_dialog(&ctx);
        self.kmeans_dialog(&ctx);
        self.t2_dialog(&ctx);
        self.sampling_dialog(&ctx);
        self.taguchi_dialog(&ctx);
        self.taguchi_design_dialog(&ctx);
        self.agreement_dialog(&ctx);
        self.sort_dialog(&ctx);
        self.stack_dialog(&ctx);
        self.unstack_dialog(&ctx);
        self.subset_dialog(&ctx);
        self.date_diff_dialog(&ctx);
        self.date_shift_dialog(&ctx);
        self.diagnostic_dialog(&ctx);
        self.power_dialog(&ctx);
        self.test2_dialog(&ctx);
        self.regression_dialog(&ctx);
        self.predict_dialog(&ctx);
        self.predict_help_window(&ctx);
        self.stepwise_dialog(&ctx);
        self.anova_dialog(&ctx);
        self.kaplan_meier_dialog(&ctx);
        self.weibull_dialog(&ctx);
        self.exponential_dialog(&ctx);
        self.lognormal_dialog(&ctx);
        self.gage_dialog(&ctx);
        self.type1_dialog(&ctx);
        self.doe_dialog(&ctx);
        self.rsm_dialog(&ctx);
        self.desirability_dialog(&ctx);
        self.scatter_dialog(&ctx);
        self.design_dialog(&ctx);
        self.box_behnken_dialog(&ctx);
        self.pb_dialog(&ctx);
        self.describe_by_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        self.assistant_dialog(&ctx);
        self.find_dialog(&ctx);
        self.format_dialog(&ctx);
        self.conditional_dialog(&ctx);
        self.about_window(&ctx);
        self.settings_info_window(&ctx);
        self.diagnostics_window(&ctx);
        self.agents_window(&ctx);
        self.licences_window(&ctx);
        self.tour_window(&ctx);
        self.batch_dialog(&ctx);
        self.samples_dialog(&ctx);
        self.graph_window(&ctx);
        self.graph_labels_window(&ctx);
        self.graph_text_window(&ctx);
        self.graph_annotation_window(&ctx);
        self.notes_window(&ctx);
        self.data_labels_window(&ctx);
        self.percentiles_window(&ctx);
        self.panel_layout_window(&ctx);
        self.axis_scale_window(&ctx);
        self.data_display_window(&ctx);
        self.reference_line_window(&ctx);
        // A modal dialog must be visible above the native graph viewport: the
        // graph is a child window and would otherwise cover the centred save
        // and option dialogs. The level returns to normal when the last
        // dialog closes.
        let dialog_open = ctx
            .data(|data| data.get_temp::<bool>(egui::Id::new("fp_dialog_open")))
            .unwrap_or(false);
        if dialog_open != self.dialogs_on_top {
            self.dialogs_on_top = dialog_open;
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if dialog_open {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            if dialog_open {
                // The graph may be a separate child viewport; bring the main
                // window (which owns the dialog) to the front as well.
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        self.persist_preferences();
    }
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ready = self.selection_is_numeric();
            if tool_icon(ui, ToolIcon::Open, "Open CSV", true).clicked() {
                self.begin_open(FileKind::Csv);
            }
            if tool_icon(ui, ToolIcon::Save, "Save CSV", true).clicked() {
                self.begin_save(FileKind::Csv);
            }
            if tool_icon(ui, ToolIcon::Undo, "Undo", true).clicked() {
                self.undo();
            }
            if tool_icon(ui, ToolIcon::Redo, "Redo", true).clicked() {
                self.redo();
            }
            let plot_label = self.catalog().label("Probability Plot");
            if tool_icon(ui, ToolIcon::Plot, plot_label, ready).clicked() {
                self.open_probability_plot_dialog();
            }
            if tool_icon(ui, ToolIcon::Clear, "Clear Session", true).clicked() {
                self.session.clear();
            }
            let samples = self.catalog().label("Samples");
            if tool_icon(ui, ToolIcon::Grid, samples, true)
                .on_hover_text("Open a sample worksheet with its analysis ready")
                .clicked()
            {
                self.open_samples_dialog();
            }
            ui.separator();
            let column_label = ui.help_label("Column:");
            let names: Vec<String> = self
                .data
                .as_ref()
                .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
                .unwrap_or_default();
            let current = self.selected.clone().unwrap_or_else(|| "—".to_owned());
            egui::ComboBox::from_id_salt("fp_column")
                .selected_text(current)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(&mut self.selected, Some(name.clone()), name.as_str());
                    }
                })
                .response
                .labelled_by(column_label.id)
                .hover_from(&column_label);

            let distribution_label = ui.help_label("Distribution:");
            egui::ComboBox::from_id_salt("fp_distribution")
                .selected_text(self.distribution.name())
                .width(110.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.distribution, DistributionKind::Normal, "Normal");
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Lognormal,
                        "Lognormal",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Weibull,
                        "Weibull",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Exponential,
                        "Exponential",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Logistic,
                        "Logistic",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Loglogistic,
                        "Loglogistic",
                    );
                    ui.selectable_value(&mut self.distribution, DistributionKind::Gamma, "Gamma");
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::ExtremeValue,
                        "Extreme Value",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Weibull3,
                        "Weibull (3-parameter)",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Lognormal3,
                        "Lognormal (3-parameter)",
                    );
                })
                .response
                .labelled_by(distribution_label.id)
                .hover_from(&distribution_label);

            ui.separator();
            ui.label(RichText::new(&self.source).italics());
        });
    }
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(170, 0, 0), error)
                    .on_hover_text(error);
            } else {
                // The column summary lives in the tooltip: on a narrow window
                // it would otherwise run under the view and zoom controls.
                let hover = if self.column_summary.is_empty() {
                    self.status.clone()
                } else {
                    format!("{}\n\n{}", self.status, self.column_summary)
                };
                ui.label(&self.status).on_hover_text(hover);
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                help_menu_button(ui, "...", |ui| {
                    if ui
                        .button("Reset zoom")
                        .on_hover_text(button_help("Reset zoom"))
                        .clicked()
                    {
                        self.zoom = 1.0;
                        ui.close();
                    }
                    ui.checkbox(&mut self.dark_mode, "Dark mode")
                        .on_hover_text(checkbox_help("Dark mode"));
                    ui.checkbox(&mut self.show_grid_lines, "Grid lines")
                        .on_hover_text(checkbox_help("Grid lines"));
                    ui.checkbox(&mut self.show_session, "Session panel")
                        .on_hover_text(checkbox_help("Session panel"));
                    ui.separator();
                    if ui
                        .button("About FreePlots")
                        .on_hover_text(button_help("About FreePlots"))
                        .clicked()
                    {
                        self.show_about = true;
                        ui.close();
                    }
                })
                .on_hover_text("View options");
                if ui
                    .button(RichText::new(">_").monospace())
                    .on_hover_text("Calculator (fx)")
                    .clicked()
                {
                    self.open_calc_dialog();
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0))
                    .on_hover_text("Interface zoom");
                if ui
                    .small_button("+")
                    .on_hover_text("Zoom in (Ctrl +)")
                    .clicked()
                {
                    self.zoom = (self.zoom * 1.1).min(max_zoom_for(ui.ctx()));
                }
                let mut percent = self.zoom * 100.0;
                ui.spacing_mut().slider_width = 80.0;
                if ui
                    .add(
                        egui::Slider::new(&mut percent, 50.0..=(max_zoom_for(ui.ctx()) * 100.0))
                            .show_value(false),
                    )
                    .on_hover_text("Interface zoom")
                    .changed()
                {
                    self.zoom = (percent / 100.0).clamp(0.5, max_zoom_for(ui.ctx()));
                }
                if ui
                    .small_button("−")
                    .on_hover_text("Zoom out (Ctrl −)")
                    .clicked()
                {
                    self.zoom = (self.zoom / 1.1).max(0.5);
                }
                if view_icon_button(
                    ui,
                    ViewIcon::Panel,
                    self.show_session,
                    "Show or hide the Session panel",
                )
                .clicked()
                {
                    self.show_session = !self.show_session;
                }
                if view_icon_button(
                    ui,
                    ViewIcon::Grid,
                    self.show_grid_lines,
                    "Show or hide the grid lines",
                )
                .clicked()
                {
                    self.show_grid_lines = !self.show_grid_lines;
                }
                if let Some(df) = &self.data {
                    ui.help_label(format!(
                        "{} × {} columns",
                        rows_label(df.n_rows()),
                        df.n_columns()
                    ))
                    .on_hover_text("Worksheet size");
                }
            });
        });
    }
    pub(super) fn sheet_tabs(&mut self, ui: &mut egui::Ui) {
        let mut switch_to: Option<usize> = None;
        let mut add = false;
        let mut scroll_to: Option<usize> = None;
        let mut rename_to: Option<usize> = None;
        let mut close_to: Option<usize> = None;
        let last = self.sheets.len().saturating_sub(1);
        let arrow = |ui: &mut egui::Ui, glyph: &str, tooltip: &str| {
            ui.add(
                egui::Button::new(RichText::new(glyph).size(11.0))
                    .min_size(egui::vec2(20.0, 22.0))
                    .corner_radius(egui::CornerRadius::same(3)),
            )
            .on_hover_text(tooltip)
            .clicked()
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if arrow(ui, "|◀", "First worksheet") {
                scroll_to = Some(0);
            }
            if arrow(ui, "◀", "Scroll the worksheet tabs left") {
                scroll_to = Some(self.tab_first.saturating_sub(1));
            }
            if arrow(ui, "▶", "Scroll the worksheet tabs right") {
                scroll_to = Some((self.tab_first + 1).min(last));
            }
            if arrow(ui, "▶|", "Last worksheet") {
                scroll_to = Some(last);
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("+").size(15.0).strong())
                        .min_size(egui::vec2(24.0, 22.0))
                        .corner_radius(egui::CornerRadius::same(3)),
                )
                .on_hover_text("Add a new worksheet")
                .clicked()
            {
                add = true;
            }
            egui::ScrollArea::horizontal()
                .id_salt("fp_sheet_tabs")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (index, sheet) in self.sheets.iter().enumerate().skip(self.tab_first) {
                            let active = index == self.active_sheet;
                            let text = RichText::new(&sheet.name)
                                .size(12.5)
                                .color(ui.visuals().text_color());
                            let response = ui
                                .add(
                                    egui::Button::new(text)
                                        .fill(if active {
                                            ui.visuals().selection.bg_fill
                                        } else {
                                            ui.visuals().widgets.inactive.weak_bg_fill
                                        })
                                        .stroke(ui.visuals().widgets.inactive.bg_stroke)
                                        .corner_radius(egui::CornerRadius {
                                            nw: 4,
                                            ne: 4,
                                            sw: 0,
                                            se: 0,
                                        })
                                        .min_size(egui::vec2(0.0, 22.0)),
                                )
                                .on_hover_text(format!("{} — click to switch", sheet.name));
                            if response.clicked() && !active {
                                switch_to = Some(index);
                            }
                            if response.double_clicked() {
                                rename_to = Some(index);
                            }
                            response.context_menu(|ui| {
                                if ui
                                    .button("Rename…")
                                    .on_hover_text(button_help("Rename…"))
                                    .clicked()
                                {
                                    rename_to = Some(index);
                                    ui.close();
                                }
                                if ui
                                    .button("Close worksheet")
                                    .on_hover_text(button_help("Close worksheet"))
                                    .clicked()
                                {
                                    close_to = Some(index);
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .button("New worksheet")
                                    .on_hover_text(button_help("New worksheet"))
                                    .clicked()
                                {
                                    add = true;
                                    ui.close();
                                }
                            });
                        }
                    });
                });
        });
        if let Some(index) = scroll_to {
            self.tab_first = index.min(last);
        }
        if let Some(index) = rename_to {
            self.open_sheet_rename(index);
        }
        if let Some(index) = close_to {
            self.close_sheet(index);
        }
        if let Some(index) = switch_to {
            self.switch_sheet(index);
        }
        if add {
            self.add_sheet();
        }
    }
}

/// Largest zoom that still fits the minimum layout (760×520 points) in the
/// physical window. Beyond it the interface is clipped with no scrollbar.
#[must_use]
pub(super) fn maximum_zoom(physical: egui::Vec2) -> f32 {
    (physical.x / 760.0).min(physical.y / 520.0).clamp(0.5, 2.5)
}

fn max_zoom_for(ctx: &egui::Context) -> f32 {
    maximum_zoom(ctx.content_rect().size() * ctx.pixels_per_point())
}
