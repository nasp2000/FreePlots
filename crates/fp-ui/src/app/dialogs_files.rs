//! File and help windows: open, save, overwrite, rename, find/format,
//! shortcuts, assistant, About, settings and the agents window.

use super::graph_edit::GraphAction;
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
            FileKind::Svg => "Open SVG",
            FileKind::GraphPdf => "Open PDF",
            FileKind::Png => "Open PNG",
            FileKind::Jpeg => "Open JPEG",
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
                    FileKind::Svg => "Path to an SVG file:",
                    FileKind::GraphPdf => "Path to a PDF file:",
                    FileKind::Png => "Path to a PNG image:",
                    FileKind::Jpeg => "Path to a JPEG image:",
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
            FileKind::Svg => "Save Graph As",
            FileKind::GraphPdf => "Print Graph",
            FileKind::Png => "Save Graph as PNG",
            FileKind::Jpeg => "Save Graph as JPEG",
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
                    FileKind::Svg => "Destination SVG file:",
                    FileKind::GraphPdf => "Destination PDF file:",
                    FileKind::Png => "Destination PNG file:",
                    FileKind::Jpeg => "Destination JPEG file:",
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
                         never records keys while a text field has focus.",
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
    /// Opens the About window (also used by the live automation bridge).
    pub fn open_about(&mut self) {
        self.show_about = true;
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
        while let Ok(action) = self.plot_action_rx.try_recv() {
            self.apply_graph_action(ctx, action);
        }
        if self.native_plot_windows && !self.any_graph_editor_open() {
            let Some(plot) = self.plot.clone() else {
                self.show_plot = false;
                return;
            };
            // The statistics block lives beside the frame, so a plot that
            // carries one opens in a wider window and keeps its plot size.
            let default_size = if plot.panels.is_empty() {
                if plot.notes.is_empty() {
                    [784.0, 620.0]
                } else {
                    [940.0, 620.0]
                }
            } else {
                [1120.0, 460.0]
            };
            let title = if plot.title.is_empty() {
                self.catalog().label("Graph").to_owned()
            } else {
                plot.title.clone()
            };
            let overlay = self.graph_overlay.clone();
            let catalog = self.catalog();
            let action_tx = self.plot_action_tx.clone();
            let brush = self.brush_marks.clone();
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
                    .with_inner_size(default_size)
                    // A graph that opens behind the worksheet hides the context
                    // menu under it; ask the window manager for the focus.
                    .with_active(true),
                move |ui, _class| {
                    if ui.ctx().input(|input| input.pointer.any_pressed()) {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    // The viewport is cleared to black; paint the theme surface
                    // first so no black band shows under a short plot.
                    let background = ui.visuals().window_fill;
                    ui.painter().rect_filled(ui.max_rect(), 0.0, background);
                    // A single plot is scaled to fit and centred; a tall
                    // multi-panel plot can outgrow a small window, so only that
                    // case needs scrolling to keep every panel reachable.
                    let mut marks = brush.lock().map(|marks| marks.clone()).unwrap_or_default();
                    let response = if plot.panels.is_empty() {
                        crate::plot_view::show_with_marks(ui, &plot, &overlay, &mut marks)
                    } else {
                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                crate::plot_view::show_with_marks(ui, &plot, &overlay, &mut marks)
                            })
                            .inner
                    };
                    if let Ok(mut shared) = brush.lock() {
                        *shared = marks;
                    }
                    response.context_menu(|ui| {
                        ui.weak(catalog.label("Brush points: Ctrl+click a point"));
                        ui.separator();
                        for (label, action) in [
                            ("Graph options…", GraphAction::Labels),
                            ("Graph text…", GraphAction::Text),
                            ("Larger text", GraphAction::LargerText),
                            ("Smaller text", GraphAction::SmallerText),
                            ("Data display…", GraphAction::DataDisplay),
                            ("Add note…", GraphAction::Annotation),
                            ("Notes…", GraphAction::Notes),
                            ("Add data labels…", GraphAction::DataLabels),
                            ("Add reference lines…", GraphAction::Reference),
                            ("Add percentile lines…", GraphAction::Percentiles),
                            ("Panel layout…", GraphAction::PanelLayout),
                            ("Axis scale…", GraphAction::AxisScale),
                            ("Toggle gridlines", GraphAction::GridToggle),
                            ("Toggle report data", GraphAction::ReportToggle),
                            ("Save graph…", GraphAction::Save),
                            ("Save as PNG…", GraphAction::SavePng),
                            ("Save as JPEG…", GraphAction::SaveJpeg),
                            ("Print graph…", GraphAction::Print),
                            ("Copy graph as SVG", GraphAction::Copy),
                            ("Clear marks", GraphAction::ClearMarks),
                        ] {
                            if action == GraphAction::PanelLayout && plot.panels.is_empty() {
                                continue;
                            }
                            if matches!(action, GraphAction::DataLabels | GraphAction::Percentiles)
                                && !plot.has_points()
                            {
                                continue;
                            }
                            if ui
                                .button(catalog.label(label))
                                .on_hover_text(catalog.label(Self::graph_action_help(action)))
                                .clicked()
                            {
                                action_tx.send(action).ok();
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .button(catalog.label("Reset graph edits"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Reset)),
                            )
                            .clicked()
                        {
                            action_tx.send(GraphAction::Reset).ok();
                            ui.close();
                        }
                    });
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        flag.store(false, Ordering::Relaxed);
                    }
                },
            );
            return;
        }
        // An open editor docks to the right, so the window grows to keep the
        // plot at its usual size next to the form.
        if self.native_plot_windows && self.plot_viewport.take().is_some() {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::from_hash_of("fp_graph"),
                egui::ViewportCommand::Close,
            );
        }
        let mut open = self.show_plot;
        let editing = self.any_graph_editor_open();
        let default_size = match self.plot.as_ref() {
            Some(plot) if !plot.panels.is_empty() => {
                if editing {
                    [1500.0, 560.0]
                } else {
                    [1120.0, 460.0]
                }
            }
            Some(_) if editing => [1180.0, 700.0],
            Some(plot) if !plot.notes.is_empty() => [940.0, 620.0],
            _ => [784.0, 620.0],
        };
        // The window carries the title of the graph it shows, so a control
        // chart is not presented as a probability plot.
        let title = self
            .plot
            .as_ref()
            .map(|plot| plot.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| self.catalog().label("Graph").to_owned());
        let mut graph_action = None;
        let brush = self.brush_marks.clone();
        let dialog_response = dialog_window(title)
            .without_clear()
            .id(egui::Id::new("graph_window"))
            .open(&mut open)
            .default_pos(egui::pos2(8.0, 64.0))
            .default_size(default_size)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(spec) = &self.plot {
                    let overlay = self.graph_overlay.clone();
                    let mut marks = brush.lock().map(|marks| marks.clone()).unwrap_or_default();
                    let available = ui.available_size();
                    let plot_width = if editing {
                        (available.x - 386.0).max(420.0)
                    } else {
                        available.x
                    };
                    let plot_ui = |ui: &mut egui::Ui| {
                        if spec.panels.is_empty() {
                            crate::plot_view::show_with_marks(ui, spec, &overlay, &mut marks)
                        } else {
                            egui::ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    crate::plot_view::show_with_marks(
                                        ui, spec, &overlay, &mut marks,
                                    )
                                })
                                .inner
                        }
                    };
                    let response = ui
                        .allocate_ui_with_layout(
                            egui::vec2(plot_width, available.y),
                            egui::Layout::top_down(egui::Align::Min),
                            plot_ui,
                        )
                        .inner;
                    if let Ok(mut shared) = brush.lock() {
                        *shared = marks;
                    }
                    response.context_menu(|ui| {
                        let catalog = self.catalog();
                        if ui
                            .button(catalog.label("Graph options…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Labels)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Labels);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Graph text…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Text)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Text);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Larger text"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::LargerText)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::LargerText);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Smaller text"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::SmallerText)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::SmallerText);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Data display…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::DataDisplay)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::DataDisplay);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Add note…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Annotation)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Annotation);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Notes…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Notes)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Notes);
                            ui.close();
                        }
                        if spec.has_points()
                            && ui
                                .button(catalog.label("Add data labels…"))
                                .on_hover_text(
                                    catalog.label(Self::graph_action_help(GraphAction::DataLabels)),
                                )
                                .clicked()
                        {
                            graph_action = Some(GraphAction::DataLabels);
                            ui.close();
                        }
                        if spec.has_points()
                            && ui
                                .button(catalog.label("Add percentile lines…"))
                                .on_hover_text(
                                    catalog
                                        .label(Self::graph_action_help(GraphAction::Percentiles)),
                                )
                                .clicked()
                        {
                            graph_action = Some(GraphAction::Percentiles);
                            ui.close();
                        }
                        if !spec.panels.is_empty()
                            && ui
                                .button(catalog.label("Panel layout…"))
                                .on_hover_text(
                                    catalog
                                        .label(Self::graph_action_help(GraphAction::PanelLayout)),
                                )
                                .clicked()
                        {
                            graph_action = Some(GraphAction::PanelLayout);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Axis scale…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::AxisScale)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::AxisScale);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Toggle gridlines"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::GridToggle)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::GridToggle);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Toggle report data"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::ReportToggle)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::ReportToggle);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Add reference lines…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Reference)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Reference);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Save graph…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Save)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Save);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Save as PNG…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::SavePng)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::SavePng);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Save as JPEG…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::SaveJpeg)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::SaveJpeg);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Print graph…"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Print)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Print);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Copy graph as SVG"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Copy)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Copy);
                            ui.close();
                        }
                        if ui
                            .button(catalog.label("Clear marks"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::ClearMarks)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::ClearMarks);
                            ui.close();
                        }
                        ui.weak(catalog.label("Brush points: Ctrl+click a point"));
                        ui.separator();
                        if ui
                            .button(catalog.label("Reset graph edits"))
                            .on_hover_text(
                                catalog.label(Self::graph_action_help(GraphAction::Reset)),
                            )
                            .clicked()
                        {
                            graph_action = Some(GraphAction::Reset);
                            ui.close();
                        }
                    });
                } else {
                    ui.help_label("No graph yet.");
                }
            });
        self.graph_dialog_rect = dialog_response.as_ref().map(|r| r.response.rect);
        self.show_plot = open;
        if let Some(action) = graph_action {
            self.apply_graph_action(ctx, action);
        }
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
    /// Opens the conditional-formatting editor.
    pub fn open_conditional_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.conditional_column = self
            .selected
            .clone()
            .filter(|name| names.contains(name))
            .or_else(|| names.first().cloned())
            .unwrap_or_default();
        self.conditional_kind = ConditionalKind::Greater;
        self.conditional_value.clear();
        self.conditional_value2.clear();
        self.conditional_style = ConditionalStyle::RedFill;
        self.show_conditional_dialog = true;
    }

    /// Adds the rule described by the dialog.
    pub fn add_conditional_rule(&mut self) {
        if self.conditional_column.is_empty() {
            self.set_error("Select the column for the rule");
            return;
        }
        let parse = |text: &str| text.trim().replace(',', ".").parse::<f64>().ok();
        let test = match self.conditional_kind {
            ConditionalKind::Greater => match parse(&self.conditional_value) {
                Some(value) => ConditionalTest::Greater(value),
                None => {
                    self.set_error("Enter a number for the limit");
                    return;
                }
            },
            ConditionalKind::Less => match parse(&self.conditional_value) {
                Some(value) => ConditionalTest::Less(value),
                None => {
                    self.set_error("Enter a number for the limit");
                    return;
                }
            },
            ConditionalKind::Between => {
                match (
                    parse(&self.conditional_value),
                    parse(&self.conditional_value2),
                ) {
                    (Some(low), Some(high)) if low <= high => ConditionalTest::Between(low, high),
                    (Some(_), Some(_)) => {
                        self.set_error("The first limit must not be above the second");
                        return;
                    }
                    _ => {
                        self.set_error("Enter the two numbers of the interval");
                        return;
                    }
                }
            }
            ConditionalKind::EqualTo => {
                if self.conditional_value.trim().is_empty() {
                    self.set_error("Enter the value to match");
                    return;
                }
                ConditionalTest::EqualTo(self.conditional_value.clone())
            }
            ConditionalKind::Contains => {
                if self.conditional_value.trim().is_empty() {
                    self.set_error("Enter the text to find");
                    return;
                }
                ConditionalTest::Contains(self.conditional_value.clone())
            }
        };
        self.conditional_rules.push(ConditionalRule {
            column: self.conditional_column.clone(),
            test,
            style: self.conditional_style,
        });
        self.error = None;
        self.status = "Conditional rule added".to_owned();
    }

    /// Removes one rule by position.
    pub fn remove_conditional_rule(&mut self, index: usize) {
        if index < self.conditional_rules.len() {
            self.conditional_rules.remove(index);
            self.status = "Conditional rule removed".to_owned();
        }
    }

    /// Removes every rule.
    pub fn clear_conditional_rules(&mut self) {
        self.conditional_rules.clear();
        self.status = "Conditional rules cleared".to_owned();
    }

    pub(super) fn conditional_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_conditional_dialog {
            return;
        }
        let mut open = self.show_conditional_dialog;
        let mut add = false;
        let mut clear = false;
        let mut remove: Option<usize> = None;
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let rules = self.conditional_rules.clone();
        let catalog = self.catalog();
        dialog_window(catalog.label("Conditional formatting"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Column:"));
                    egui::ComboBox::from_id_salt("conditional_column")
                        .selected_text(if self.conditional_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.conditional_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.conditional_column,
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
                    let label = ui.help_label(catalog.label("Condition:"));
                    egui::ComboBox::from_id_salt("conditional_kind")
                        .selected_text(match self.conditional_kind {
                            ConditionalKind::Greater => catalog.label("Greater than"),
                            ConditionalKind::Less => catalog.label("Less than"),
                            ConditionalKind::Between => catalog.label("Between"),
                            ConditionalKind::EqualTo => catalog.label("Equal to"),
                            ConditionalKind::Contains => catalog.label("Contains"),
                        })
                        .show_ui(ui, |ui| {
                            for (kind, name) in [
                                (ConditionalKind::Greater, "Greater than"),
                                (ConditionalKind::Less, "Less than"),
                                (ConditionalKind::Between, "Between"),
                                (ConditionalKind::EqualTo, "Equal to"),
                                (ConditionalKind::Contains, "Contains"),
                            ] {
                                ui.selectable_value(
                                    &mut self.conditional_kind,
                                    kind,
                                    catalog.label(name),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Value:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.conditional_value).desired_width(120.0),
                    )
                    .labelled_by(label.id);
                    if self.conditional_kind == ConditionalKind::Between {
                        ui.label(catalog.label("and"));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.conditional_value2)
                                .desired_width(120.0),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Style:"));
                    egui::ComboBox::from_id_salt("conditional_style")
                        .selected_text(match self.conditional_style {
                            ConditionalStyle::RedFill => catalog.label("Red fill"),
                            ConditionalStyle::YellowFill => catalog.label("Yellow fill"),
                            ConditionalStyle::GreenFill => catalog.label("Green fill"),
                            ConditionalStyle::RedText => catalog.label("Red text"),
                            ConditionalStyle::Bold => catalog.label("Bold"),
                        })
                        .show_ui(ui, |ui| {
                            for (style, name) in [
                                (ConditionalStyle::RedFill, "Red fill"),
                                (ConditionalStyle::YellowFill, "Yellow fill"),
                                (ConditionalStyle::GreenFill, "Green fill"),
                                (ConditionalStyle::RedText, "Red text"),
                                (ConditionalStyle::Bold, "Bold"),
                            ] {
                                ui.selectable_value(
                                    &mut self.conditional_style,
                                    style,
                                    catalog.label(name),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(catalog.label(
                    "The first rule that matches a cell wins; the rules follow the column name and are kept between sessions.",
                ));
                ui.separator();
                if rules.is_empty() {
                    ui.weak(catalog.label("No rules yet."));
                } else {
                    for (index, rule) in rules.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui
                                .help_button(catalog.label("Remove"))
                                .on_hover_text(catalog.label("Remove this rule"))
                                .clicked()
                            {
                                remove = Some(index);
                            }
                            ui.label(rule_description(rule));
                        });
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Add rule")).clicked() {
                        add = true;
                    }
                    if ui.help_button(catalog.label("Clear rules")).clicked() {
                        clear = true;
                    }
                    if ui.help_button(catalog.label("Close")).clicked() {
                        self.show_conditional_dialog = false;
                    }
                });
            });
        self.show_conditional_dialog = open && self.show_conditional_dialog;
        if add {
            self.add_conditional_rule();
        }
        if let Some(index) = remove {
            self.remove_conditional_rule(index);
        }
        if clear {
            self.clear_conditional_rules();
        }
    }

    /// Opens the batch-folder dialog.
    pub fn open_batch_dialog(&mut self) {
        self.batch_column = self.selected.clone().unwrap_or_default();
        self.show_batch_dialog = true;
    }

    /// Asks the desktop runner to read the folder and summarise the column.
    pub fn run_batch_dialog(&mut self) {
        let folder = self.batch_folder.trim().to_owned();
        let column = self.batch_column.trim().to_owned();
        if folder.is_empty() {
            self.set_error("Enter the folder with the CSV files");
            return;
        }
        if column.is_empty() {
            self.set_error("Select the column to summarise");
            return;
        }
        self.batch_request = Some(BatchRequest { folder, column });
        self.show_batch_dialog = false;
        self.error = None;
        self.status = "Reading the batch folder…".to_owned();
    }

    /// Takes the pending batch request (the desktop runner reads the folder).
    pub fn take_batch_request(&mut self) -> Option<BatchRequest> {
        self.batch_request.take()
    }

    /// Appends the batch summary once the runner has read the files.
    pub fn apply_batch(
        &mut self,
        files: Vec<(String, Result<fp_core::data::DataFrame, String>)>,
        column: &str,
    ) {
        if files.is_empty() {
            self.set_error("No CSV files found in the folder");
            return;
        }
        let result = fp_core::batch::describe(&files, column);
        self.append_result(&result);
        self.status = format!("Batch summary: {} file(s), column {column}", files.len());
        self.error = None;
    }

    pub(super) fn batch_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_batch_dialog {
            return;
        }
        let mut open = self.show_batch_dialog;
        let mut run = false;
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| {
                        matches!(
                            column.dtype(),
                            fp_core::data::DType::Float | fp_core::data::DType::Integer
                        )
                    })
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let catalog = self.catalog();
        dialog_window(catalog.label("Batch descriptive statistics"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Folder:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.batch_folder)
                            .desired_width(300.0)
                            .hint_text(catalog.label("Folder with .csv files")),
                    )
                    .labelled_by(label.id);
                    if ui.help_button(catalog.label("Browse…")).clicked() {
                        self.request_folder_browse(catalog.label("Choose the folder with the CSV files"));
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Column:"));
                    egui::ComboBox::from_id_salt("batch_column")
                        .selected_text(if self.batch_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.batch_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.batch_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(catalog.label(
                    "Every .csv in the folder is summarised in one Session table; unreadable files are listed with the reason.",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Run")).clicked() {
                        run = true;
                    }
                    if ui.help_button(catalog.label("Cancel")).clicked() {
                        self.show_batch_dialog = false;
                    }
                });
            });
        self.show_batch_dialog = open && self.show_batch_dialog;
        if run {
            self.run_batch_dialog();
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

/// Reader-facing summary of a conditional rule.
fn rule_description(rule: &ConditionalRule) -> String {
    let condition = match &rule.test {
        ConditionalTest::Greater(limit) => format!("> {limit}"),
        ConditionalTest::Less(limit) => format!("< {limit}"),
        ConditionalTest::Between(low, high) => format!("{low} … {high}"),
        ConditionalTest::EqualTo(value) => format!("= {value}"),
        ConditionalTest::Contains(value) => format!("contains \"{value}\""),
    };
    let style = match rule.style {
        ConditionalStyle::RedFill => "red fill",
        ConditionalStyle::YellowFill => "yellow fill",
        ConditionalStyle::GreenFill => "green fill",
        ConditionalStyle::RedText => "red text",
        ConditionalStyle::Bold => "bold",
    };
    format!("{} {condition} → {style}", rule.column)
}
