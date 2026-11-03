//! File and help windows: open, save, overwrite, rename, find/format,
//! shortcuts, assistant, About, settings and the agents window.

use super::*;

impl FreePlotsApp {
    pub(super) fn open_window(&mut self, ctx: &egui::Context) {
        if !self.show_open {
            return;
        }
        let mut open = self.show_open;
        let kind = self.open_kind;
        let title = match kind {
            FileKind::Csv => "Open CSV",
            FileKind::Project => "Open Project",
            FileKind::Html => "Open HTML",
            FileKind::Pdf => "Open PDF",
            FileKind::Xlsx => "Open Excel",
        };
        dialog_window(title)
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(match kind {
                    FileKind::Csv => "Path to a CSV file:",
                    FileKind::Project => "Path to a .fpp project:",
                    FileKind::Html => "Path to an HTML file:",
                    FileKind::Pdf => "Path to a PDF file:",
                    FileKind::Xlsx => "Path to an Excel workbook:",
                });
                ui.horizontal(|ui| {
                    let response = ui
                        .text_edit_singleline(&mut self.open_path)
                        .on_hover_text("Path of the file to open; Browse… opens the system dialog");
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.help_button(self.catalog().label("Browse…")).clicked() {
                        self.request_browse(kind, false);
                    }
                    if ui.help_button(self.catalog().label("Open")).clicked() || submitted {
                        let path = self.open_path.trim();
                        if !path.is_empty() {
                            if kind == FileKind::Csv {
                                self.preview_request = Some(path.to_owned());
                            } else {
                                self.open_request = Some(OpenRequest {
                                    path: path.to_owned(),
                                    kind,
                                    sheet: None,
                                    csv_options: None,
                                });
                            }
                        }
                        self.show_open = false;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_open = false;
                    }
                });
                ui.small(
                    "Browse opens the native file dialog of the operating system; the file itself \
                     is read by the desktop runner, not by the UI crate.",
                );
            });
        self.show_open = open && self.show_open;
    }
    pub(super) fn overwrite_window(&mut self, ctx: &egui::Context) {
        if !self.show_overwrite {
            return;
        }
        let mut open = self.show_overwrite;
        let path = self
            .overwrite_pending
            .as_ref()
            .map(|(path, _, _)| path.clone())
            .unwrap_or_default();
        dialog_window("Replace existing file?")
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(format!("{path} already exists. Replace it?"));
                ui.horizontal(|ui| {
                    if ui.help_button("Replace").clicked() {
                        self.confirm_overwrite();
                    }
                    if ui.help_button("Cancel").clicked() {
                        self.overwrite_pending = None;
                        self.show_overwrite = false;
                    }
                });
            });
        if !open && self.show_overwrite {
            // The window was closed with its X: keep the file untouched.
            self.overwrite_pending = None;
            self.show_overwrite = false;
        }
    }
    pub(super) fn save_window(&mut self, ctx: &egui::Context) {
        if !self.show_save {
            return;
        }
        let mut open = self.show_save;
        let kind = self.save_kind;
        let title = match kind {
            FileKind::Csv => "Save CSV As",
            FileKind::Project => "Save Project As",
            FileKind::Html => "Export Session HTML",
            FileKind::Pdf => "Export Session PDF",
            FileKind::Xlsx => "Save Excel Workbook",
        };
        dialog_window(title)
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(match kind {
                    FileKind::Csv => "Destination CSV file:",
                    FileKind::Project => "Destination .fpp project:",
                    FileKind::Html => "Destination HTML file:",
                    FileKind::Pdf => "Destination PDF file:",
                    FileKind::Xlsx => "Destination Excel workbook:",
                });
                ui.horizontal(|ui| {
                    let response = ui
                        .text_edit_singleline(&mut self.save_path)
                        .on_hover_text("Destination file; Browse… opens the system dialog");
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.help_button(self.catalog().label("Browse…")).clicked() {
                        self.request_browse(kind, true);
                    }
                    if kind == FileKind::Csv {
                        ui.help_checkbox(
                            &mut self.save_decimal_comma,
                            "Portuguese Excel format (`;` and decimal comma)",
                        );
                    }
                    if ui.help_button(self.catalog().label("Save")).clicked() || submitted {
                        let path = self.save_path.trim().to_owned();
                        if !path.is_empty() {
                            self.request_write(path, kind, self.save_decimal_comma);
                        }
                        self.show_save = false;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_save = false;
                    }
                });
                ui.small(
                    "Browse opens the native file dialog of the operating system; the file itself \
                     is written by the desktop runner, not by the UI crate.",
                );
            });
        self.show_save = open && self.show_save;
    }
    pub(super) fn sheet_rename_window(&mut self, ctx: &egui::Context) {
        if !self.show_sheet_rename {
            return;
        }
        let mut open = self.show_sheet_rename;
        dialog_window(self.catalog().label("Rename Worksheet…"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                let label = ui.help_label("New name:");
                let response = ui
                    .text_edit_singleline(&mut self.sheet_rename_text)
                    .labelled_by(label.id)
                    .hover_from(&label);
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Rename")).clicked() || submitted {
                        self.commit_sheet_rename();
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_sheet_rename = false;
                    }
                });
            });
        self.show_sheet_rename = open && self.show_sheet_rename;
    }
    pub(super) fn rename_window(&mut self, ctx: &egui::Context) {
        if !self.show_rename {
            return;
        }
        let mut open = self.show_rename;
        dialog_window(self.catalog().label("Rename Column"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                let label = ui.help_label("New name:");
                let response = ui
                    .text_edit_singleline(&mut self.rename_text)
                    .labelled_by(label.id)
                    .hover_from(&label);
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Rename")).clicked() || submitted {
                        self.commit_rename_column();
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_rename = false;
                    }
                });
            });
        self.show_rename = open && self.show_rename;
    }
    pub(super) fn find_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_find_dialog {
            return;
        }
        let mut open = self.show_find_dialog;
        let mut close = false;
        let mut next = false;
        let mut replace = false;
        let mut replace_all = false;
        let mut submitted = false;
        dialog_window(self.catalog().label("Find and Replace"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Find what:");
                    let response = ui
                        .text_edit_singleline(&mut self.find_text)
                        .labelled_by(label.id)
                        .hover_from(&label);
                    submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Replace with:");
                    ui.text_edit_singleline(&mut self.replace_text)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(&mut self.find_match_case, "Match case");
                ui.help_checkbox(&mut self.find_whole_cell, "Match the whole cell");
                if !self.find_message.is_empty() {
                    ui.small(&self.find_message);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.find_text.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Find Next"))
                        .on_hover_text("Select the next cell that matches")
                        .clicked()
                    {
                        next = true;
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Replace"))
                        .on_hover_text("Replace this cell, then find the next match")
                        .clicked()
                    {
                        replace = true;
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Replace All"))
                        .on_hover_text("Replace every match in this worksheet")
                        .clicked()
                    {
                        replace_all = true;
                    }
                    if ui
                        .help_button("Close")
                        .on_hover_text("Close without replacing")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        self.show_find_dialog = open && !close;
        if next || submitted {
            self.find_next();
        }
        if replace {
            self.replace_current();
        }
        if replace_all {
            self.replace_all();
        }
    }
    pub(super) fn format_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_format_dialog {
            return;
        }
        let mut open = self.show_format_dialog;
        let mut close = false;
        let mut apply = false;
        let mut remove = false;
        let column = self.selected.clone().unwrap_or_else(|| "—".to_owned());
        dialog_window(self.catalog().label("Format Cells"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Column: {column}"));
                ui.horizontal(|ui| {
                    let label = ui.help_label("Decimal places:");
                    ui.add(egui::DragValue::new(&mut self.format_decimals).range(0..=8))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(&mut self.format_thousands, "Thousands separator");
                ui.help_checkbox(&mut self.format_bold, "Bold");
                ui.horizontal(|ui| {
                    ui.help_label("Alignment:");
                    ui.radio_value(&mut self.format_align, CellAlign::Left, "Left")
                        .on_hover_text(checkbox_help("Left"));
                    ui.radio_value(&mut self.format_align, CellAlign::Center, "Center")
                        .on_hover_text(checkbox_help("Center"));
                    ui.radio_value(&mut self.format_align, CellAlign::Right, "Right")
                        .on_hover_text(checkbox_help("Right"));
                });
                ui.small(
                    "The format changes the worksheet display only; the stored values keep \
                          full precision.",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button("Apply").clicked() {
                        apply = true;
                    }
                    if ui
                        .help_button("Remove format")
                        .on_hover_text("Show the column with the default format")
                        .clicked()
                    {
                        remove = true;
                    }
                    if ui.help_button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        self.show_format_dialog = open && !close;
        if apply {
            self.apply_column_format();
            self.show_format_dialog = false;
        }
        if remove {
            self.clear_column_format();
            self.show_format_dialog = false;
        }
    }
    pub(super) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = self.show_shortcuts;
        let shortcuts: &[(&str, &str)] = &[
            (
                "Ctrl+N / Ctrl+O / Ctrl+S",
                "New worksheet, open and save as",
            ),
            ("Ctrl+P", "Print the Session report"),
            ("Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z", "Undo and redo"),
            ("Ctrl+C / Ctrl+X / Ctrl+V", "Copy, cut and paste"),
            ("Ctrl+F", "Find and replace"),
            ("Ctrl+D / Ctrl+R", "Fill the selection down and right"),
            ("Ctrl+A", "Select the whole worksheet"),
            ("Delete / Backspace", "Clear the selected cells"),
            ("Arrows / Shift+Arrows", "Move or extend the selection"),
            ("Tab", "Next column"),
            ("Enter", "Commit the edit and move down"),
            ("Escape", "Cancel the edit or close the dialog"),
            ("Ctrl + / Ctrl − / Ctrl 0", "Zoom in, out and reset"),
            ("Double-click", "Edit a cell"),
            ("Right-click", "Context menu of the cell, header or tab"),
        ];
        dialog_window(self.catalog().label("Keyboard shortcuts"))
            .without_clear()
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .show(ui, |ui| {
                        egui::Grid::new("shortcuts_grid")
                            .num_columns(2)
                            .spacing([18.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (keys, action) in shortcuts {
                                    ui.label(RichText::new(*keys).monospace().size(12.5));
                                    ui.label(self.catalog().label(action));
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.label(
                    RichText::new(self.catalog().label(
                        "Ctrl and Shift combinations use the keyboard layout in use; the grid \
                         never captures keys while a text field has focus.",
                    ))
                    .small()
                    .color(ui.visuals().text_color()),
                );
            });
        self.show_shortcuts = open;
    }
    pub(super) fn assistant_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_assistant_dialog {
            return;
        }
        let mut open = self.show_assistant_dialog;
        let mut choice: Option<u8> = None;
        let goals = [
            "Describe my data",
            "Compare two groups",
            "Compare three or more groups",
            "Test a proportion",
            "Check a process (control chart)",
            "Measure process capability",
            "Fit a regression",
            "Design an experiment",
            "Test normality",
        ];
        dialog_window(self.catalog().label("Assistant"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.help_label("What do you want to do?");
                for (index, goal) in goals.iter().enumerate() {
                    if ui
                        .add_sized(
                            [ui.available_width(), 24.0],
                            egui::Button::new(self.catalog().label(goal)),
                        )
                        .clicked()
                    {
                        choice = Some(index as u8);
                    }
                }
            });
        self.show_assistant_dialog = open && choice.is_none();
        match choice {
            Some(0) => self.run_descriptive_statistics(),
            Some(1) => self.open_t_test_dialog(TTestKind::TwoSample),
            Some(2) => self.open_anova_dialog(),
            Some(3) => self.open_test2_dialog(Test2Kind::OneProportion),
            Some(4) => {
                if let Some(column) = &self.selected {
                    self.chart_column = column.clone();
                }
                self.chart_kind = ControlChartKind::IndividualsMovingRange;
                self.show_chart_dialog = true;
            }
            Some(5) => {
                if let Some(column) = &self.selected {
                    self.cap_column = column.clone();
                }
                self.show_cap_dialog = true;
            }
            Some(6) => self.open_regression_dialog(),
            Some(7) => self.open_doe_dialog(),
            Some(8) => self.open_probability_plot_dialog(),
            _ => {}
        }
    }
    pub(super) fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = self.show_about;
        dialog_window(self.catalog().label("About FreePlots"))
            .without_clear()
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    about_formula(ui);
                });
                ui.add_space(16.0);
                ui.label(RichText::new(format!("FreePlots Engine {}", fp_core::VERSION)).strong());
                ui.label(
                    RichText::new(self.catalog().label(
                        "Statistics and analysis software written in Rust by AI, accessible to everyone.",
                    ))
                    .size(13.0),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(self.catalog().label(
                        "Independent project, not affiliated with or endorsed by any commercial statistical software vendor.",
                    ))
                    .size(12.0),
                );
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(self.catalog().label("MIT license"))
                            .size(12.0)
                            .color(ui.visuals().text_color()),
                    );
                });
            });
        self.show_about = open;
    }
    pub(super) fn settings_info_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings_info {
            return;
        }
        let mut open = self.show_settings_info;
        let path = self
            .preferences_path()
            .map(|path| path.display().to_string());
        let preferences = self.preferences();
        let error = self.preferences_error().map(str::to_owned);
        let mut copy: Option<String> = None;
        dialog_window(self.catalog().label("Where the settings are stored"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(self.catalog().label(
                    "FreePlots keeps these options in a small JSON file: language, theme, zoom, the Session panel, the grid lines and the native plot windows. The file is written as soon as one of them changes.",
                ));
                ui.add_space(6.0);
                match &path {
                    Some(path) => {
                        ui.horizontal(|ui| {
                            ui.monospace(path);
                            if ui.help_button(self.catalog().label("Copy path")).clicked() {
                                copy = Some(path.clone());
                            }
                        });
                        ui.weak(self.catalog().label(
                            "The program uses FreePlots-data next to the executable when it can write there, so a USB stick keeps its own settings; otherwise the user data directory is used.",
                        ));
                    }
                    None => {
                        ui.weak(self.catalog().label(
                            "This build has no settings file (the command line does not keep preferences).",
                        ));
                    }
                }
                ui.separator();
                egui::Grid::new("settings_values")
                    .num_columns(2)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(self.catalog().label("Language"));
                        ui.label(match preferences.language.as_str() {
                            "pt" => "Português",
                            _ => "English",
                        });
                        ui.end_row();
                        ui.label(self.catalog().label("Theme"));
                        ui.label(if preferences.dark_mode {
                            self.catalog().label("Dark")
                        } else {
                            self.catalog().label("Light")
                        });
                        ui.end_row();
                        ui.label(self.catalog().label("Zoom"));
                        ui.label(format!("{:.0} %", preferences.zoom * 100.0));
                        ui.end_row();
                        ui.label(self.catalog().label("Session panel"));
                        ui.label(if preferences.show_session {
                            self.catalog().label("Shown")
                        } else {
                            self.catalog().label("Hidden")
                        });
                        ui.end_row();
                        ui.label(self.catalog().label("Grid lines"));
                        ui.label(if preferences.show_grid_lines {
                            self.catalog().label("Shown")
                        } else {
                            self.catalog().label("Hidden")
                        });
                        ui.end_row();
                        ui.label(self.catalog().label("Native plot windows"));
                        ui.label(if preferences.native_plot_windows {
                            self.catalog().label("On")
                        } else {
                            self.catalog().label("Off")
                        });
                        ui.end_row();
                    });
                if let Some(error) = &error {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 70, 60),
                        format!("{} {error}", self.catalog().label("The file could not be written:")),
                    );
                }
                ui.separator();
                ui.weak(self.catalog().label(
                    "Removing the file restores the defaults; it is plain JSON and can be edited by hand.",
                ));
            });
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        self.show_settings_info = open;
    }
    pub(super) fn agents_window(&mut self, ctx: &egui::Context) {
        if !self.show_agents_dialog {
            return;
        }
        let mut open = self.show_agents_dialog;
        let mut copy: Option<String> = None;
        let launch: [(&str, &str); 4] = [
            (
                "Desktop tools over stdio (headless worksheet)",
                "FreePlots --mcp",
            ),
            (
                "Same tools over HTTP on this machine",
                "FreePlots --mcp-http 127.0.0.1:8767",
            ),
            (
                "Live window: start the bridge, then the proxy",
                "FreePlots --bridge 127.0.0.1:8767  then  FreePlots --mcp-live 127.0.0.1:8767",
            ),
            (
                "Token for a bridge reachable from the network",
                "--token <secret>",
            ),
        ];
        let client = "{\n  \"mcpServers\": {\n    \"freeplots\": {\n      \"command\": \"FreePlots\",\n      \"args\": [\"--mcp\"]\n    }\n  }\n}";
        dialog_window(self.catalog().label("Assistant for agents"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(620.0)
            .show(ctx, |ui| {
                ui.label(self.catalog().label(
                    "FreePlots embeds no model: you choose the MCP client (the AI assistant you already use). The client opens files, runs analyses and reads the Session through the tools below.",
                ));
                ui.separator();
                ui.strong(self.catalog().label("Launch lines"));
                egui::Grid::new("agents_launch")
                    .num_columns(3)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for (description, line) in launch {
                            ui.label(self.catalog().label(description));
                            ui.monospace(line);
                            if ui.small_button(self.catalog().label("Copy")).clicked() {
                                copy = Some(line.to_owned());
                            }
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
                ui.strong(self.catalog().label("Client configuration"));
                ui.horizontal(|ui| {
                    if ui
                        .help_button(self.catalog().label("Copy configuration"))
                        .clicked()
                    {
                        copy = Some(client.to_owned());
                    }
                    ui.weak(self.catalog().label(
                        "An stdio entry for a client that starts FreePlots itself.",
                    ));
                });
                ui.monospace(client);
                ui.separator();
                ui.label(self.catalog().label(
                    "Tools: open a CSV, describe, capability, control chart, regression, ANOVA, normality, probability plot, Session, status and screenshot. The live window also accepts menu paths, clicks, text and scripts.",
                ));
                ui.weak(self.catalog().label(
                    "The bridge is for a trusted network: it can change the open worksheet. Keep --token private when the address is not loopback.",
                ));
            });
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        self.show_agents_dialog = open;
    }
    pub(super) fn graph_window(&mut self, ctx: &egui::Context) {
        if !self.show_plot {
            if self.plot_viewport.take().is_some() {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("fp_graph"),
                    egui::ViewportCommand::Close,
                );
            }
            return;
        }
        if self.native_plot_windows {
            let Some(plot) = self.plot.clone() else {
                self.show_plot = false;
                return;
            };
            let default_size = if plot.panels.is_empty() {
                [784.0, 620.0]
            } else {
                [1120.0, 460.0]
            };
            let title = if plot.title.is_empty() {
                self.catalog().label("Graph").to_owned()
            } else {
                plot.title.clone()
            };
            let overlay = self.graph_overlay.clone();
            let flag = self
                .plot_viewport
                .get_or_insert_with(|| Arc::new(AtomicBool::new(true)))
                .clone();
            if !flag.load(Ordering::Relaxed) {
                self.plot_viewport = None;
                self.show_plot = false;
                return;
            }
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("fp_graph"),
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size(default_size),
                move |ui, _class| {
                    // The viewport is cleared to black; paint the theme surface
                    // first so no black band shows under a short plot.
                    let background = ui.visuals().window_fill;
                    ui.painter().rect_filled(ui.max_rect(), 0.0, background);
                    // Tall multi-panel plots can be larger than the window on a
                    // small screen; scrolling keeps every panel reachable.
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::plot_view::show_with(ui, &plot, &overlay);
                        });
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        flag.store(false, Ordering::Relaxed);
                    }
                },
            );
            return;
        }
        let mut open = self.show_plot;
        let default_size = if self
            .plot
            .as_ref()
            .is_some_and(|plot| !plot.panels.is_empty())
        {
            [1120.0, 460.0]
        } else {
            [784.0, 620.0]
        };
        // The window carries the title of the graph it shows, so a control
        // chart is not presented as a probability plot.
        let title = self
            .plot
            .as_ref()
            .map(|plot| plot.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| self.catalog().label("Graph").to_owned());
        dialog_window(title)
            .without_clear()
            .id(egui::Id::new("graph_window"))
            .open(&mut open)
            .default_pos(egui::pos2(8.0, 64.0))
            .default_size(default_size)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(spec) = &self.plot {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::plot_view::show_with(ui, spec, &self.graph_overlay);
                        });
                } else {
                    ui.help_label("No graph yet.");
                }
            });
        self.show_plot = open;
    }
    pub(super) fn csv_options_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_csv_options {
            return;
        }
        let mut open = self.show_csv_options;
        let mut confirm = false;
        let mut cancel = false;
        let preview = self.csv_preview.clone();
        dialog_window(self.catalog().label("CSV Options"))
            .open(&mut open)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.help_label(format!("File: {}", self.csv_path));
                if let Some(preview) = &preview {
                    ui.help_label(format!("Detected encoding: {}", preview.encoding));
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            egui::ScrollArea::both()
                                .max_height(160.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&preview.text).monospace().small(),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                });
                        });
                }
                ui.add_space(4.0);
                egui::Grid::new("csv_options_grid")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        let delimiter_label = ui.help_label("Delimiter:");
                        egui::ComboBox::from_id_salt("csv_delimiter")
                            .width(160.0)
                            .selected_text(match self.csv_delimiter {
                                b';' => "Semicolon (;)",
                                b'\t' => "Tab",
                                _ => "Comma (,)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.csv_delimiter, b',', "Comma (,)");
                                ui.selectable_value(&mut self.csv_delimiter, b';', "Semicolon (;)");
                                ui.selectable_value(&mut self.csv_delimiter, b'\t', "Tab");
                            })
                            .response
                            .labelled_by(delimiter_label.id)
                            .hover_from(&delimiter_label);
                        ui.end_row();
                        let _ = ui.help_label("Numbers:");
                        ui.help_checkbox(
                            &mut self.csv_decimal_comma,
                            "Comma is the decimal separator",
                        );
                        ui.end_row();
                        let _ = ui.help_label("Header:");
                        ui.help_checkbox(&mut self.csv_header, "First row is the header");
                        ui.end_row();
                    });
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.help_button(self.catalog().label("Open")).clicked() {
                        confirm = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_csv_options = open && !confirm && !cancel;
        if confirm {
            self.open_request = Some(OpenRequest {
                path: self.csv_path.clone(),
                kind: FileKind::Csv,
                sheet: None,
                csv_options: Some(CsvOptions {
                    delimiter: self.csv_delimiter,
                    decimal_comma: self.csv_decimal_comma,
                    has_header: self.csv_header,
                }),
            });
            self.show_csv_options = false;
        }
        if cancel {
            self.show_csv_options = false;
        }
    }
    pub(super) fn sheet_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sheet_dialog {
            return;
        }
        let mut open = self.show_sheet_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Choose Worksheet"))
            .without_clear()
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.help_label("The workbook has several worksheets:");
                for name in &self.sheet_names {
                    ui.selectable_value(&mut self.sheet_choice, name.clone(), name.as_str());
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Open")).clicked() {
                    run = true;
                }
            });
        self.show_sheet_dialog = open && !run;
        if run {
            self.open_request = Some(OpenRequest {
                path: self.sheet_path.clone(),
                kind: FileKind::Xlsx,
                sheet: Some(self.sheet_choice.clone()),
                csv_options: None,
            });
            self.show_sheet_dialog = false;
        }
    }
    pub fn open_find_dialog(&mut self) {
        self.show_find_dialog = true;
        self.find_message.clear();
    }
    pub fn open_format_dialog(&mut self) {
        let Some(name) = self.selected.clone() else {
            return;
        };
        let format = self.formats.get(&name).copied().unwrap_or_default();
        self.format_decimals = format.decimals;
        self.format_thousands = format.thousands;
        self.format_bold = format.bold;
        self.format_align = format.align;
        self.show_format_dialog = true;
    }
}
