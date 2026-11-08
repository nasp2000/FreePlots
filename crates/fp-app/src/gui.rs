// SPDX-License-Identifier: MIT
//! Native desktop runner (opt-in `desktop` feature).
//!
//! This is the only place where the window system and file I/O meet the UI.
//! `fp-ui` is pure egui; the application reads the chosen CSV here and hands an
//! in-memory frame to the UI.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::{Value, json};

use eframe::egui;
use fp_core::workflow::{AnalysisSpec, DistributionKind};
use fp_ui::{FileKind, FreePlotsApp};

use crate::bridge::{Bridge, Command};

/// Result of a finished native file browser: the "save" flag and the path.
type BrowseResult = (bool, Option<PathBuf>);

struct Desktop {
    app: FreePlotsApp,
    /// Live automation bridge, when started with `--bridge`.
    bridge: Option<Arc<Bridge>>,
    /// Screenshot waiting for the next frame's image (`inline` adds base64).
    pending_shot: Option<(Sender<Value>, PathBuf, bool)>,
    /// Pending file browser, running on its own thread.
    browse: Option<Receiver<BrowseResult>>,
    /// Pending folder browser, running on its own thread.
    folder_browse: Option<Receiver<Option<PathBuf>>>,
    /// The black caption is applied once, after the native window exists.
    dark_caption: bool,
    /// Data directory, for the diagnostics window.
    data_dir: Option<PathBuf>,
    /// Last error already written to the log.
    last_error: Option<String>,
}

impl eframe::App for Desktop {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.serve_paste();
        self.serve_dropped(raw_input);
        let commands = self
            .bridge
            .as_ref()
            .map(|bridge| bridge.drain())
            .unwrap_or_default();
        for command in commands {
            self.handle_bridge(ctx, raw_input, command);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.dark_caption {
            #[cfg(target_os = "windows")]
            crate::platform::apply_dark_caption();
            self.dark_caption = true;
        }

        // A finished file browser fills the path field of the dialog.
        if let Some(receiver) = &self.browse
            && let Ok((save, path)) = receiver.try_recv()
        {
            self.browse = None;
            if let Some(path) = path {
                self.app
                    .set_browsed_path(path.to_string_lossy().into_owned(), save);
            }
        }
        // A new file browser runs on its own thread: the native dialog owns
        // its message loop and must not block the egui frame.
        if let Some(request) = self.app.take_browse_request() {
            let (sender, receiver) = channel();
            self.browse = Some(receiver);
            let save = request.save;
            std::thread::spawn(move || {
                let path = crate::platform::browse_file(&request.title, &request.filters, save);
                let _ = sender.send((save, path));
            });
        }
        // A finished folder browser fills the batch-folder field.
        if let Some(receiver) = &self.folder_browse
            && let Ok(path) = receiver.try_recv()
        {
            self.folder_browse = None;
            if let Some(path) = path {
                self.app
                    .set_browsed_folder(path.to_string_lossy().into_owned());
            }
        }
        if let Some(request) = self.app.take_folder_request() {
            let (sender, receiver) = channel();
            self.folder_browse = Some(receiver);
            std::thread::spawn(move || {
                let path = crate::platform::browse_folder(&request.title);
                let _ = sender.send(path);
            });
        }

        self.app.show(ui);

        if self.app.autosave_due()
            && let Some(path) = self.app.autosave_path().map(std::path::Path::to_path_buf)
        {
            let written =
                fp_core::io::to_project_json_sheets(&self.app.project_sheets(), "autosave.fpp")
                    .map_err(|error| error.to_string())
                    .and_then(|json| crate::platform::write_text(&path, &json));
            if let Err(error) = written {
                self.app.set_status(format!("Autosave failed: {error}"));
            }
            self.app.note_autosave();
        }
        if self.app.take_restore_request()
            && let Some(path) = self.app.autosave_path().map(std::path::Path::to_path_buf)
        {
            match crate::platform::load_project_sheets(&path) {
                Ok(sheets) => self
                    .app
                    .apply_restored(sheets, format!("{} [recovered]", path.display())),
                Err(error) => self
                    .app
                    .set_error(format!("Cannot restore the autosave: {error}")),
            }
        }
        if self.app.take_discard_autosave()
            && let Some(path) = self.app.autosave_path().map(std::path::Path::to_path_buf)
        {
            let _ = crate::platform::remove_file(&path);
        }
        if let Some(url) = self.app.take_open_url_request()
            && let Err(error) = crate::platform::open_url(&url)
        {
            self.app.set_error(error);
        }
        if self.app.take_open_data_dir_request()
            && let Some(dir) = &self.data_dir
            && let Err(error) = crate::platform::open_path(dir)
        {
            self.app.set_error(error);
        }
        let error = self.app.last_error().map(str::to_owned);
        if error != self.last_error {
            if let Some(error) = &error
                && let Some(log) = self.app.diagnostics_log_path()
            {
                crate::platform::log_line(log, &format!("error: {error}"));
            }
            self.last_error = error;
        }

        let title = format!("FreePlots — {}", self.app.source());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(title));

        if let Some(path) = self.app.take_print_request() {
            let bytes = fp_core::pdf::session_pdf(
                "FreePlots Session",
                self.app.session_text(),
                self.app.plot(),
            );
            if let Err(error) = crate::platform::write_bytes(&path, &bytes)
                .and_then(|()| crate::platform::open_path(&path))
            {
                self.app.set_error(error);
            }
        }

        if let Some(path) = self.app.take_preview_request() {
            match crate::platform::read_csv_preview(std::path::Path::new(&path)) {
                Ok(preview) => self.app.set_csv_preview(path, preview),
                Err(error) => self.app.set_error(error),
            }
        }

        if let Some(request) = self.app.take_open_request() {
            let path = std::path::Path::new(&request.path);
            // `Ok(None)` means "nothing to load": the interface asked which
            // worksheet to use and will emit a new request with the answer.
            let result: Result<Option<(fp_core::data::DataFrame, String, Vec<String>)>, String> =
                match request.kind {
                    FileKind::Csv => match request.csv_options {
                        Some(options) => crate::platform::read_csv_with_options(path, options),
                        None => crate::platform::read_csv(path),
                    }
                    .map(|(df, encoding)| {
                        Some((df, format!("{} [{encoding}]", request.path), Vec::new()))
                    }),
                    FileKind::Project => crate::platform::load_project_sheets(path).map(|sheets| {
                        if sheets.len() > 1 {
                            self.app.load_project_sheets(sheets, request.path.clone());
                            None
                        } else {
                            sheets
                                .into_iter()
                                .next()
                                .map(|(_, df)| (df, request.path.clone(), Vec::new()))
                        }
                    }),
                    FileKind::Html
                    | FileKind::Pdf
                    | FileKind::Svg
                    | FileKind::GraphPdf
                    | FileKind::Png
                    | FileKind::Jpeg => {
                        Err("HTML, PDF, SVG, PNG and JPEG are export-only formats".to_owned())
                    }
                    FileKind::Xlsx => match &request.sheet {
                        Some(sheet) => {
                            crate::platform::read_xlsx_sheet_with_warnings(path, Some(sheet)).map(
                                |(df, warnings)| {
                                    Some((df, format!("{} [{sheet}]", request.path), warnings))
                                },
                            )
                        }
                        None => match crate::platform::xlsx_sheet_names(path) {
                            Ok(names) if names.len() > 1 => {
                                self.app.set_workbook_sheets(&request.path, names);
                                Ok(None)
                            }
                            Ok(_) => crate::platform::read_xlsx_with_warnings(path).map(
                                |(df, warnings)| {
                                    Some((df, format!("{} [xlsx]", request.path), warnings))
                                },
                            ),
                            Err(error) => Err(error),
                        },
                    },
                };
            match result {
                Ok(Some((df, source, warnings))) => {
                    self.app.set_data(df, source);
                    for warning in warnings {
                        self.app.push_warning(warning);
                    }
                }
                Ok(None) => {}
                Err(e) => self.app.set_error(e),
            }
        }

        if let Some(request) = self.app.take_save_request() {
            let path = std::path::Path::new(&request.path);
            let result = match request.kind {
                FileKind::Html => crate::platform::write_text(
                    path,
                    &fp_core::report::session_html(self.app.session_text()),
                ),
                FileKind::Pdf => {
                    let bytes = fp_core::pdf::session_pdf(
                        "FreePlots Session",
                        self.app.session_text(),
                        self.app.plot(),
                    );
                    crate::platform::write_bytes(path, &bytes)
                }
                FileKind::Svg => match self.app.plot() {
                    Some(plot) => crate::platform::write_text(
                        path,
                        &fp_core::plot::render_svg_with(plot, self.app.graph_overlay()),
                    ),
                    None => Err("No graph to save".to_owned()),
                },
                FileKind::GraphPdf => match self.app.plot() {
                    Some(plot) => {
                        let title = if plot.title.is_empty() {
                            "Graph"
                        } else {
                            plot.title.as_str()
                        };
                        let bytes = fp_core::pdf::session_pdf(title, "", Some(plot));
                        crate::platform::write_bytes(path, &bytes)
                    }
                    None => Err("No graph to print".to_owned()),
                },
                FileKind::Png | FileKind::Jpeg => match self.app.plot() {
                    Some(plot) => {
                        let svg = fp_core::plot::render_svg_with(plot, self.app.graph_overlay());
                        let bytes = match request.kind {
                            FileKind::Png => crate::export_image::svg_to_png(&svg, 2.0),
                            _ => crate::export_image::svg_to_jpeg(&svg, 2.0, 90),
                        };
                        bytes.and_then(|bytes| crate::platform::write_bytes(path, &bytes))
                    }
                    None => Err("No graph to save".to_owned()),
                },
                FileKind::Csv | FileKind::Project | FileKind::Xlsx => match self.app.data() {
                    Some(df) => match request.kind {
                        FileKind::Csv => {
                            crate::platform::write_csv(path, df, request.decimal_comma)
                        }
                        FileKind::Project => crate::platform::save_project_sheets(
                            path,
                            &self.app.project_sheets(),
                            &request.path,
                        ),
                        FileKind::Xlsx => {
                            crate::platform::write_bytes(path, &fp_core::xlsx::workbook(df))
                        }
                        FileKind::Html
                        | FileKind::Pdf
                        | FileKind::Svg
                        | FileKind::GraphPdf
                        | FileKind::Png
                        | FileKind::Jpeg => unreachable!(),
                    },
                    None => Err("No worksheet to save".to_owned()),
                },
            };
            match result {
                Ok(()) if request.kind == FileKind::Html || request.kind == FileKind::Pdf => {
                    self.app.mark_session_exported(&request.path);
                }
                Ok(())
                    if request.kind == FileKind::Svg
                        || request.kind == FileKind::GraphPdf
                        || request.kind == FileKind::Png
                        || request.kind == FileKind::Jpeg =>
                {
                    self.app.mark_graph_saved(&request.path);
                }
                Ok(()) => self.app.mark_saved(&request.path, request.kind),
                Err(e) => self.app.set_error(e),
            }
        }

        // A dirty project asks for a recovery copy before the close prompt.
        if self.app.take_recovery_request()
            && let Some(path) = self.app.recovery_path()
        {
            match fp_core::io::to_project_json_sheets(&self.app.project_sheets(), "recovery.fpp") {
                Ok(json) => {
                    if let Err(error) = crate::platform::write_text(path, &json) {
                        eprintln!("freeplots: recovery snapshot: {error}");
                    }
                }
                Err(error) => eprintln!("freeplots: recovery snapshot: {error}"),
            }
        }

        if let Some(request) = self.app.take_batch_request() {
            let files = crate::platform::read_csv_folder(std::path::Path::new(&request.folder));
            self.app.apply_batch(files, &request.column);
        }

        if let Some((reply, path, inline)) = self.pending_shot.take() {
            let shot = ui.ctx().input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let png = crate::bridge::encode_png(
                    image.width() as u32,
                    image.height() as u32,
                    image.as_raw(),
                );
                let ok = std::fs::write(&path, &png).is_ok();
                let mut answer = json!({
                    "ok": ok,
                    "path": path.display().to_string(),
                    "width": image.width(),
                    "height": image.height(),
                    "pixels_per_point": ui.ctx().pixels_per_point()
                });
                if inline {
                    use base64::Engine as _;
                    answer["png_base64"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(&png));
                }
                let _ = reply.send(answer);
            } else {
                self.pending_shot = Some((reply, path, inline));
            }
        }
    }
}

impl Desktop {
    /// Opens files dropped on the window (CSV, Excel or a project).
    fn serve_dropped(&mut self, raw_input: &mut egui::RawInput) {
        for file in raw_input.dropped_files.drain(..) {
            self.app.open_dropped(&file.path().display().to_string());
        }
    }

    /// Fills a pending paste request from the system clipboard.
    fn serve_paste(&mut self) {
        if !self.app.take_paste_request() {
            return;
        }
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
            Ok(text) if !text.trim().is_empty() => {
                self.app.paste_tsv(&text);
                self.app.set_status("Pasted from the clipboard");
            }
            _ => self.app.paste_internal(),
        }
    }

    fn handle_bridge(
        &mut self,
        ctx: &egui::Context,
        raw_input: &mut egui::RawInput,
        command: Command,
    ) {
        let request = &command.request;
        let name = request.get("cmd").and_then(Value::as_str).unwrap_or("");
        let reply = match name {
            "click" | "right_click" | "double_click" => {
                let button = if name == "right_click" {
                    egui::PointerButton::Secondary
                } else {
                    egui::PointerButton::Primary
                };
                inject_click(raw_input, request, button);
                if name == "double_click" {
                    inject_click(raw_input, request, button);
                }
                json!({"ok": true})
            }
            "move" => {
                let pos = position(request);
                raw_input.events.push(egui::Event::PointerMoved(pos));
                json!({"ok": true})
            }
            "key" => match request
                .get("key")
                .and_then(Value::as_str)
                .and_then(egui::Key::from_name)
            {
                Some(key) => {
                    raw_input.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    raw_input.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                    json!({"ok": true})
                }
                None => json!({"ok": false, "error": "unknown key"}),
            },
            "paste" => {
                // Mirrors what egui-winit does for Ctrl+V: a Paste event
                // carrying the clipboard text.
                match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                    Ok(text) if !text.trim().is_empty() => {
                        raw_input
                            .events
                            .push(egui::Event::Paste(text.replace("\r\n", "\n")));
                        json!({"ok": true})
                    }
                    Ok(_) => json!({"ok": false, "error": "the clipboard is empty"}),
                    Err(e) => json!({"ok": false, "error": e.to_string()}),
                }
            }
            "text" => {
                let text = request.get("text").and_then(Value::as_str).unwrap_or("");
                raw_input.events.push(egui::Event::Text(text.to_owned()));
                json!({"ok": true})
            }
            "describe" => {
                let column = text(request, "column");
                let spec = match request.get("by").and_then(Value::as_str) {
                    Some(by) => AnalysisSpec::descriptive_statistics_by(column.clone(), by),
                    None => AnalysisSpec::descriptive_statistics(column.clone()),
                };
                self.reply_spec(spec, format!("Descriptive statistics: {column}"))
            }
            "probability" => {
                let column = text(request, "column");
                let distribution = distribution(request);
                let spec =
                    AnalysisSpec::probability_plot(column.clone()).with_distribution(distribution);
                self.reply_spec(spec, format!("Probability plot: {column}"))
            }
            "capability" => {
                let column = text(request, "column");
                let lsl = request.get("lsl").and_then(Value::as_f64);
                let usl = request.get("usl").and_then(Value::as_f64);
                let mut spec = AnalysisSpec::capability(column.clone(), lsl, usl);
                if let Some(target) = request.get("target").and_then(Value::as_f64) {
                    spec = spec.with_capability_target(Some(target));
                }
                if let Some(confidence) = request.get("confidence").and_then(Value::as_f64) {
                    spec = spec.with_capability_confidence(confidence);
                }
                if request.get("distribution").is_some() {
                    spec = spec.with_capability_distribution(distribution(request));
                }
                self.reply_spec(spec, format!("Capability: {column}"))
            }
            "controlchart" => {
                let column = text(request, "column");
                let chart = request
                    .get("chart")
                    .and_then(Value::as_str)
                    .and_then(fp_core::spc::ControlChartKind::parse)
                    .unwrap_or(fp_core::spc::ControlChartKind::IndividualsMovingRange);
                let subgroup = request
                    .get("subgroup")
                    .and_then(Value::as_u64)
                    .map_or(1, |value| value as usize);
                let mut spec = AnalysisSpec::control_chart(column.clone(), chart, subgroup);
                if let Some(group) = request.get("subgroup_column").and_then(Value::as_str) {
                    spec = spec.with_subgroup_column(Some(group.to_owned()));
                }
                if let Some(phase) = request.get("phase_column").and_then(Value::as_str) {
                    spec = spec.with_phase_column(Some(phase.to_owned()));
                }
                self.reply_spec(spec, format!("Control chart: {column}"))
            }
            "regression" => {
                let response = text(request, "response");
                let predictors: Vec<String> = request
                    .get("predictors")
                    .and_then(Value::as_str)
                    .map(|raw| raw.split(',').map(|name| name.trim().to_owned()).collect())
                    .unwrap_or_default();
                let spec = AnalysisSpec::regression(response.clone(), predictors);
                self.reply_spec(spec, format!("Regression: {response}"))
            }
            "anova" => {
                let response = text(request, "response");
                let factor = text(request, "factor");
                let spec = AnalysisSpec::anova(response.clone(), factor.clone());
                self.reply_spec(spec, format!("One-way ANOVA: {response} by {factor}"))
            }
            "normality" => {
                let column = text(request, "column");
                let spec = AnalysisSpec::normality_test(column.clone());
                self.reply_spec(spec, format!("Normality test: {column}"))
            }
            "about" => {
                self.app.open_about();
                json!({"ok": true})
            }
            "theme" => {
                let dark = request.get("dark").and_then(Value::as_bool).unwrap_or(true);
                self.app.set_dark_mode(dark);
                json!({"ok": true, "dark": dark})
            }
            "status" => json!({
                "ok": true,
                "status": self.app.status(),
                "source": self.app.source(),
            }),
            "session" => json!({"ok": true, "session": self.app.session_text()}),
            "open_csv" => {
                let path = request.get("path").and_then(Value::as_str).unwrap_or("");
                match crate::platform::read_csv(std::path::Path::new(path)) {
                    Ok((df, encoding)) => {
                        self.app.set_data(df, format!("{path} [{encoding}]"));
                        json!({"ok": true})
                    }
                    Err(error) => json!({"ok": false, "error": error}),
                }
            }
            "plot_svg" => match self.app.plot() {
                Some(plot) => json!({
                    "ok": true,
                    "svg": fp_core::plot::render_svg_with(plot, self.app.graph_overlay()),
                }),
                None => json!({"ok": false, "error": "no graph to export"}),
            },
            "screenshot" => {
                let path = request.get("path").and_then(Value::as_str).map_or_else(
                    || std::env::temp_dir().join("freeplots-bridge.png"),
                    PathBuf::from,
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                let inline = request
                    .get("inline")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.pending_shot = Some((command.reply, path, inline));
                return;
            }
            other => json!({"ok": false, "error": format!("unknown command {other:?}")}),
        };
        let _ = command.reply.send(reply);
        // A bridge command may open a dialog or change the plot; draw it now
        // instead of waiting for the next pointer or keyboard event.
        ctx.request_repaint();
    }
}

impl Desktop {
    fn reply_spec(&mut self, spec: AnalysisSpec, label: String) -> Value {
        let before = self.app.session_text().len();
        match self.app.run_spec_blocking(spec, label) {
            Ok(()) => {
                let session = self.app.session_text();
                let report = session.get(before..).unwrap_or(session).to_owned();
                json!({
                    "ok": true,
                    "status": self.app.status(),
                    "session": report,
                })
            }
            Err(error) => json!({"ok": false, "error": error}),
        }
    }
}

fn text(request: &Value, key: &str) -> String {
    request
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn distribution(request: &Value) -> DistributionKind {
    match request
        .get("distribution")
        .and_then(Value::as_str)
        .unwrap_or("normal")
        .to_ascii_lowercase()
        .as_str()
    {
        "lognormal" | "ln" => DistributionKind::Lognormal,
        "weibull" | "w" => DistributionKind::Weibull,
        "exponential" | "exp" => DistributionKind::Exponential,
        "logistic" | "log" => DistributionKind::Logistic,
        "loglogistic" | "llog" => DistributionKind::Loglogistic,
        "gamma" | "g" => DistributionKind::Gamma,
        "extremevalue" | "ev" | "gumbel" => DistributionKind::ExtremeValue,
        "weibull3" | "weibull-3" => DistributionKind::Weibull3,
        "lognormal3" | "lognormal-3" => DistributionKind::Lognormal3,
        _ => DistributionKind::Normal,
    }
}

fn position(request: &Value) -> egui::Pos2 {
    egui::pos2(
        request.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        request.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    )
}

fn inject_click(raw_input: &mut egui::RawInput, request: &Value, button: egui::PointerButton) {
    let pos = position(request);
    raw_input.events.push(egui::Event::PointerMoved(pos));
    raw_input.events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    raw_input.events.push(egui::Event::PointerButton {
        pos,
        button,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
}

/// Opens the desktop application.
///
/// Arguments after `--gui`:
/// * `[file.csv]` — load a worksheet at startup;
/// * `--distribution <normal|lognormal|weibull|exponential|logistic|loglogistic|gamma|extremevalue>`
///   — default analysis family;
/// * `--plot` — run the default probability plot immediately (demos/tests);
/// * `--renderer <glow|wgpu>` — force a rendering backend. By default Windows
///   uses `wgpu` (D3D12/Vulkan) and other platforms use `glow`; the environment
///   variable `FREEPLOTS_RENDERER` overrides the default.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut path: Option<PathBuf> = None;
    let mut plot = false;
    let mut distribution: Option<DistributionKind> = None;
    let mut bridge_address: Option<String> = None;
    let mut bridge_token: Option<String> = None;
    let mut renderer: Option<eframe::Renderer> = None;
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--plot" => plot = true,
            "--renderer" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--renderer requires glow or wgpu".to_owned())?;
                renderer = Some(match raw.as_str() {
                    "glow" => eframe::Renderer::Glow,
                    "wgpu" => eframe::Renderer::Wgpu,
                    other => {
                        return Err(format!("invalid --renderer: {other} (use glow or wgpu)"));
                    }
                });
            }
            "--bridge" => {
                bridge_address = Some(
                    it.next()
                        .ok_or_else(|| "--bridge requires an address (host:port)".to_owned())?
                        .clone(),
                );
            }
            "--token" => {
                bridge_token = Some(
                    it.next()
                        .ok_or_else(|| "--token requires a secret".to_owned())?
                        .clone(),
                );
            }
            "--gui" => return Err("--gui must be the first argument".to_owned()),
            "--distribution" => {
                let raw = it
                    .next()
                    .ok_or_else(|| "--distribution requires a distribution name".to_owned())?;
                distribution = Some(match raw.as_str() {
                    "normal" | "n" => DistributionKind::Normal,
                    "lognormal" | "ln" => DistributionKind::Lognormal,
                    "weibull" | "w" => DistributionKind::Weibull,
                    "exponential" | "exp" => DistributionKind::Exponential,
                    "logistic" | "log" => DistributionKind::Logistic,
                    "loglogistic" | "llog" => DistributionKind::Loglogistic,
                    "gamma" | "g" => DistributionKind::Gamma,
                    "extremevalue" | "ev" | "gumbel" => DistributionKind::ExtremeValue,
                    "weibull3" | "weibull-3" => DistributionKind::Weibull3,
                    "lognormal3" | "lognormal-3" => DistributionKind::Lognormal3,
                    other => {
                        return Err(format!(
                            "invalid --distribution: {other} (use normal, lognormal, weibull, \
                             exponential, logistic, loglogistic, gamma, extremevalue, weibull3 or \
                             lognormal3)"
                        ));
                    }
                });
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }

    let mut app = FreePlotsApp::new();
    // Preferences and the diagnostic log live in the data directory (next to
    // the executable when it is writable, otherwise the user data directory).
    let data_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
        .and_then(|dir| crate::platform::resolve_data_dir(&dir).ok())
        .map(|data| data.path);
    let log_path = data_dir
        .as_ref()
        .map(|dir| dir.join(crate::platform::LOG_NAME));
    if let Some(log) = &log_path {
        crate::platform::install_panic_hook(log.clone());
        crate::platform::log_line(
            log,
            &format!(
                "FreePlots {} starting on {}",
                fp_core::VERSION,
                std::env::consts::OS
            ),
        );
    }
    if let Some(path) = data_dir.as_ref().map(|dir| dir.join("settings.json")) {
        app.set_preferences_path(path);
    }
    // A newer autosave is offered before the first file is opened.
    let autosave = app.autosave_path().map(std::path::Path::to_path_buf);
    if let Some(autosave) = autosave
        && autosave.exists()
    {
        let newer = match path.as_deref() {
            Some(project)
                if project
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("fpp")) =>
            {
                crate::platform::is_newer(&autosave, project)
            }
            _ => true,
        };
        if newer {
            app.offer_restore(autosave);
        } else {
            // The open project is newer than the autosave: it is stale.
            let _ = crate::platform::remove_file(&autosave);
        }
    }
    app.maybe_show_tour();
    app.set_background_analyses(true);
    app.set_distribution(distribution.unwrap_or(DistributionKind::Normal));
    if let Some(path) = path {
        let (df, encoding) = crate::platform::read_csv(&path)?;
        app.set_data(df, format!("{} [{encoding}]", path.display()));
        if plot {
            app.run_probability_plot();
        }
    }

    let renderer = match renderer {
        Some(renderer) => renderer,
        None => match std::env::var("FREEPLOTS_RENDERER") {
            Ok(raw) => match raw.as_str() {
                "glow" => eframe::Renderer::Glow,
                "wgpu" => eframe::Renderer::Wgpu,
                other => {
                    return Err(format!(
                        "invalid FREEPLOTS_RENDERER: {other} (use glow or wgpu)"
                    ));
                }
            },
            Err(_) if cfg!(windows) => eframe::Renderer::Wgpu,
            Err(_) => eframe::Renderer::Glow,
        },
    };
    if let (Some(dir), Some(log)) = (&data_dir, &log_path) {
        let renderer_name = match renderer {
            eframe::Renderer::Glow => "glow",
            eframe::Renderer::Wgpu => "wgpu",
        };
        app.set_diagnostics(
            format!(
                "FreePlots {}\nOS: {}\nRenderer: {}\nData folder: {}\nLog: {}",
                fp_core::VERSION,
                std::env::consts::OS,
                renderer_name,
                dir.display(),
                log.display()
            ),
            log.clone(),
        );
    }
    let options = eframe::NativeOptions {
        renderer,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([760.0, 520.0])
            .with_clamp_size_to_monitor_size(true)
            .with_icon(window_icon())
            .with_title("FreePlots"),
        ..Default::default()
    };
    let bridge = match bridge_address {
        Some(address) => Some(Bridge::start(&address, bridge_token)?),
        None => None,
    };
    eframe::run_native(
        "FreePlots",
        options,
        Box::new(|_cc| {
            Ok(Box::new(Desktop {
                app,
                bridge,
                pending_shot: None,
                browse: None,
                folder_browse: None,
                dark_caption: false,
                data_dir,
                last_error: None,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

/// The window and taskbar icon: 64×64 RGBA, generated by `tools/make_icon.py`.
fn window_icon() -> egui::IconData {
    egui::IconData {
        rgba: include_bytes!("../assets/freeplots-64.rgba").to_vec(),
        width: 64,
        height: 64,
    }
}
