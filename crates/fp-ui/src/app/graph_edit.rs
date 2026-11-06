//! Graph presentation edits: labels, subtitle, footer and reference lines.

use super::*;
use fp_core::plot::{
    Annotation, AnnotationColor, DataLabelMode, GraphOverlay, ReferenceAxis, ReferenceLine,
};

/// Action chosen from the graph context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphAction {
    Labels,
    Text,
    Annotation,
    Notes,
    DataLabels,
    Percentiles,
    PanelLayout,
    AxisScale,
    DataDisplay,
    GridToggle,
    ReportToggle,
    Reference,
    Save,
    SavePng,
    SaveJpeg,
    Print,
    Copy,
    ClearMarks,
    Reset,
}

impl FreePlotsApp {
    /// Opens the label editor with the current texts.
    pub fn open_graph_labels(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        if self.graph_auto.is_none() {
            self.graph_auto = Some((
                plot.title.clone(),
                plot.x_axis.label.clone(),
                plot.y_axis.label.clone(),
            ));
        }
        self.graph_label_title = plot.title.clone();
        self.graph_label_x = plot.x_axis.label.clone();
        self.graph_label_y = plot.y_axis.label.clone();
        self.graph_label_subtitle = self.graph_overlay.subtitle.clone().unwrap_or_default();
        self.graph_label_footer = self.graph_overlay.footer.clone().unwrap_or_default();
        self.show_graph_labels = true;
    }

    /// Duplicates one note, offsetting it slightly so both stay visible.
    pub fn duplicate_note(&mut self, index: usize) {
        if let Some(note) = self.graph_overlay.annotations.get(index).cloned() {
            let mut copy = note;
            copy.x = (copy.x + 0.02).clamp(0.0, 1.0);
            copy.y = (copy.y + 0.02).clamp(0.0, 1.0);
            self.graph_overlay.annotations.push(copy);
            self.status = "Note duplicated".to_owned();
        }
    }

    /// Removes one note.
    pub fn delete_note(&mut self, index: usize) {
        if index < self.graph_overlay.annotations.len() {
            self.graph_overlay.annotations.remove(index);
            self.status = "Note removed".to_owned();
        }
    }

    /// Rotates one note by `delta` degrees, within -90..90.
    pub fn rotate_note(&mut self, index: usize, delta: f32) {
        if let Some(note) = self.graph_overlay.annotations.get_mut(index) {
            let angle = if note.angle.is_finite() {
                note.angle
            } else {
                0.0
            };
            note.angle = (angle + delta).clamp(-90.0, 90.0);
            self.status = format!("Note angle {:.0}°", note.angle);
        }
    }

    /// Moves a note to the end of the list, so it is drawn on top.
    pub fn bring_note_to_front(&mut self, index: usize) {
        if index < self.graph_overlay.annotations.len() {
            let note = self.graph_overlay.annotations.remove(index);
            self.graph_overlay.annotations.push(note);
            self.status = "Note brought to front".to_owned();
        }
    }

    /// Opens the data-label editor with the current choice.
    pub fn open_data_labels(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        if !plot.has_points() {
            self.set_error("This graph has no points to label");
            return;
        }
        self.data_label_mode = self.graph_overlay.data_labels.unwrap_or_default();
        self.show_data_labels = true;
    }

    /// Applies the data labels (or removes them when `remove` is set).
    pub fn apply_data_labels(&mut self, remove: bool) {
        self.graph_overlay.data_labels = (!remove).then_some(self.data_label_mode);
        self.show_data_labels = false;
        self.status = if remove {
            "Data labels removed".to_owned()
        } else {
            "Data labels added".to_owned()
        };
    }

    /// Opens the percentile-lines dialog.
    pub fn open_percentiles(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        if !plot.has_points() {
            self.set_error("This graph has no points to take percentiles from");
            return;
        }
        self.percentile_axis = ReferenceAxis::Horizontal;
        self.percentile_values = "1, 5, 50, 95, 99".to_owned();
        self.show_percentiles = true;
    }

    /// Adds one reference line per percentile of the plotted values.
    pub fn add_percentile_lines(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        let mut percentiles: Vec<f64> = Vec::new();
        for piece in self
            .percentile_values
            .split([',', ';', ' '])
            .filter(|piece| !piece.trim().is_empty())
        {
            match piece.trim().replace(',', ".").parse::<f64>() {
                Ok(value) if value > 0.0 && value < 100.0 => percentiles.push(value),
                _ => {
                    self.set_error("Each percentile must be a number between 0 and 100");
                    return;
                }
            }
        }
        if percentiles.is_empty() {
            self.set_error("Enter at least one percentile");
            return;
        }
        let mut values: Vec<f64> = match self.percentile_axis {
            ReferenceAxis::Vertical => plot.points.iter().map(|point| point.x).collect(),
            ReferenceAxis::Horizontal => plot.points.iter().map(|point| point.y).collect(),
        };
        if values.is_empty() {
            self.set_error("The graph has no points to take percentiles from");
            return;
        }
        values.sort_by(f64::total_cmp);
        let mut added = 0usize;
        for percentile in percentiles {
            let rank = percentile / 100.0 * (values.len() - 1) as f64;
            let low = rank.floor() as usize;
            let high = rank.ceil() as usize;
            let weight = rank - low as f64;
            let value = values[low] * (1.0 - weight) + values[high] * weight;
            self.graph_overlay.reference_lines.push(ReferenceLine {
                axis: self.percentile_axis,
                value,
                label: Some(format!("P{percentile:.0}")),
            });
            added += 1;
        }
        self.show_percentiles = false;
        self.status = format!("{added} percentile line(s) added");
    }

    pub(super) fn data_display_window(&mut self, ctx: &egui::Context) {
        if !self.show_data_display {
            return;
        }
        let mut open = self.show_data_display;
        let catalog = self.catalog();
        self.graph_editor_dialog(catalog.label("Data display"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.checkbox(&mut self.graph_overlay.show_symbols, catalog.label("Symbols"));
                ui.checkbox(&mut self.graph_overlay.show_lines, catalog.label("Lines"));
                ui.checkbox(
                    &mut self.graph_overlay.show_bands,
                    catalog.label("Bands / bars"),
                );
                ui.small(catalog.label(
                    "Unchecking an element hides it in the window and in the saved SVG/PDF; the settings are kept between sessions.",
                ));
                ui.separator();
                if ui.help_button(catalog.label("Show all")).clicked() {
                    self.graph_overlay.show_symbols = true;
                    self.graph_overlay.show_lines = true;
                    self.graph_overlay.show_bands = true;
                }
                if ui.help_button(catalog.label("Close")).clicked() {
                    self.show_data_display = false;
                }
            });
        self.show_data_display = open && self.show_data_display;
    }

    /// Opens the manual axis-scale editor.
    pub fn open_axis_scale(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        let text = |range: Option<(f64, f64)>, low: bool| match range {
            Some((low_value, high_value)) => {
                if low {
                    format!("{low_value}")
                } else {
                    format!("{high_value}")
                }
            }
            None => String::new(),
        };
        self.axis_x_min = text(self.graph_overlay.x_range, true);
        self.axis_x_max = text(self.graph_overlay.x_range, false);
        self.axis_y_min = text(self.graph_overlay.y_range, true);
        self.axis_y_max = text(self.graph_overlay.y_range, false);
        let _ = plot;
        self.show_axis_scale = true;
    }

    /// Parses one min/max pair; `Ok(None)` means automatic.
    fn parse_range(low: &str, high: &str, name: &str) -> Result<Option<(f64, f64)>, String> {
        let low = low.trim();
        let high = high.trim();
        if low.is_empty() && high.is_empty() {
            return Ok(None);
        }
        if low.is_empty() || high.is_empty() {
            return Err(format!("Enter both limits of the {name} axis, or neither"));
        }
        let parse = |value: &str| value.replace(',', ".").parse::<f64>().ok();
        match (parse(low), parse(high)) {
            (Some(low), Some(high)) if low < high => Ok(Some((low, high))),
            (Some(_), Some(_)) => Err(format!("The {name} minimum must be below the maximum")),
            _ => Err(format!("The {name} limits must be numbers")),
        }
    }

    /// Applies the manual axis limits, or clears them.
    pub fn apply_axis_scale(&mut self, automatic: bool) {
        if automatic {
            self.graph_overlay.x_range = None;
            self.graph_overlay.y_range = None;
            self.show_axis_scale = false;
            self.status = "Axis scale automatic".to_owned();
            return;
        }
        let x = Self::parse_range(&self.axis_x_min, &self.axis_x_max, "x");
        let y = Self::parse_range(&self.axis_y_min, &self.axis_y_max, "y");
        let (x, y) = match (x, y) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(error), _) | (_, Err(error)) => {
                self.set_error(&error);
                return;
            }
        };
        if let Some(plot) = &self.plot {
            if x.is_some() && matches!(plot.x_axis.scale, fp_core::plot::Scale::Probability(_)) {
                self.set_error("Manual limits are not available on a probability axis");
                return;
            }
            if y.is_some() && matches!(plot.y_axis.scale, fp_core::plot::Scale::Probability(_)) {
                self.set_error("Manual limits are not available on a probability axis");
                return;
            }
            if !plot.panels.is_empty() && (x.is_some() || y.is_some()) {
                self.set_error("Manual limits are available on single plots, not on panels");
                return;
            }
        }
        self.graph_overlay.x_range = x;
        self.graph_overlay.y_range = y;
        self.show_axis_scale = false;
        self.error = None;
        self.status = "Axis scale updated".to_owned();
    }

    /// Opens the panel layout editor (multi-panel graphs only).
    pub fn open_panel_layout(&mut self) {
        let Some(plot) = &self.plot else {
            self.set_error("No graph to edit");
            return;
        };
        if plot.panels.is_empty() {
            self.set_error("This graph has no panels");
            return;
        }
        self.panel_columns_draft = plot.panel_columns.max(1);
        self.show_panel_layout = true;
    }

    /// Applies the number of panel columns.
    pub fn apply_panel_layout(&mut self) {
        let Some(plot) = &mut self.plot else {
            return;
        };
        plot.panel_columns = self.panel_columns_draft.clamp(1, 6);
        self.show_panel_layout = false;
        self.status = format!("Panel layout: {} column(s)", plot.panel_columns);
    }

    /// Opens the graph text editor with the current typeface and size.
    pub fn open_graph_text(&mut self) {
        if self.plot.is_none() {
            self.set_error("No graph to edit");
            return;
        }
        self.graph_text_family = self.graph_overlay.font_family;
        self.graph_text_size = self.graph_overlay.text_size(12.0);
        self.show_graph_text = true;
    }

    /// Opens the annotation editor for a free text on the plot.
    pub fn open_graph_annotation(&mut self) {
        if self.plot.is_none() {
            self.set_error("No graph to edit");
            return;
        }
        self.annotation_text.clear();
        self.annotation_x = 5.0;
        self.annotation_y = 5.0;
        self.annotation_size = 12.0;
        self.annotation_color = AnnotationColor::Black;
        self.annotation_angle = 0.0;
        self.show_graph_annotation = true;
    }

    /// Places the annotation at the chosen fraction of the plot area.
    pub fn add_graph_annotation(&mut self) {
        let text = self.annotation_text.trim();
        if text.is_empty() {
            self.set_error("Type the note to place on the graph");
            return;
        }
        let x = if self.annotation_x.is_finite() {
            self.annotation_x.clamp(0.0, 100.0) / 100.0
        } else {
            0.05
        };
        let y = if self.annotation_y.is_finite() {
            self.annotation_y.clamp(0.0, 100.0) / 100.0
        } else {
            0.05
        };
        self.graph_overlay.annotations.push(Annotation {
            text: text.to_owned(),
            x,
            y,
            size: if self.annotation_size.is_finite() {
                self.annotation_size.clamp(6.0, 48.0)
            } else {
                12.0
            },
            color: self.annotation_color,
            angle: if self.annotation_angle.is_finite() {
                self.annotation_angle
            } else {
                0.0
            },
        });
        self.show_graph_annotation = false;
        self.error = None;
        self.status = "Note added to the graph".to_owned();
    }

    /// Applies the typeface and size to every text drawn on the graph.
    pub fn apply_graph_text(&mut self) {
        self.preview_graph_text();
        self.show_graph_text = false;
        self.status = "Graph text updated".to_owned();
    }

    /// Applies the label editor.
    pub fn apply_graph_labels(&mut self) {
        self.preview_graph_labels();
        self.show_graph_labels = false;
        self.status = "Graph options updated".to_owned();
    }

    /// Presentation edits of the current graph (subtitle, footer, lines).
    #[must_use]
    pub fn graph_overlay(&self) -> &GraphOverlay {
        &self.graph_overlay
    }

    /// Records that the graph was written to `path` (the worksheet is
    /// untouched, so the source label stays as it was).
    pub fn mark_graph_saved(&mut self, path: &str) {
        self.status = format!("Graph saved to {path}");
        self.error = None;
    }

    /// Opens the save dialog for the current graph as SVG.
    pub fn begin_plot_save(&mut self) {
        if self.plot.is_none() {
            self.set_error("No graph to save");
            return;
        }
        self.save_kind = FileKind::Svg;
        self.save_path = "graph.svg".to_owned();
        self.show_save = true;
    }

    /// Opens the save dialog for a PNG or JPEG of the current graph.
    pub fn begin_graph_image(&mut self, kind: FileKind) {
        if self.plot.is_none() {
            self.set_error("No graph to save");
            return;
        }
        self.save_kind = kind;
        self.save_path = match kind {
            FileKind::Jpeg => "graph.jpg".to_owned(),
            _ => "graph.png".to_owned(),
        };
        self.show_save = true;
    }

    /// Opens the save dialog for a one-page PDF with the current graph.
    pub fn begin_graph_pdf(&mut self) {
        if self.plot.is_none() {
            self.set_error("No graph to print");
            return;
        }
        self.save_kind = FileKind::GraphPdf;
        self.save_path = "graph.pdf".to_owned();
        self.show_save = true;
    }

    /// Opens the reference-line dialog.
    pub fn open_reference_line(&mut self) {
        if self.plot.is_none() {
            self.set_error("No graph to edit");
            return;
        }
        self.reference_value.clear();
        self.reference_label.clear();
        self.reference_axis = ReferenceAxis::Vertical;
        self.show_reference_line = true;
    }

    /// Adds the reference line described by the dialog.
    pub fn add_reference_line(&mut self) {
        let Ok(value) = self.reference_value.trim().replace(',', ".").parse::<f64>() else {
            self.set_error("The reference value must be a number");
            return;
        };
        if !value.is_finite() {
            self.set_error("The reference value must be finite");
            return;
        }
        let label = self.reference_label.trim();
        self.graph_overlay.reference_lines.push(ReferenceLine {
            axis: self.reference_axis,
            value,
            label: (!label.is_empty()).then(|| label.to_owned()),
        });
        self.show_reference_line = false;
        self.status = format!("Reference line at {value}");
    }

    /// Removes every presentation edit and restores the automatic labels.
    pub fn reset_graph_edits(&mut self) {
        self.graph_overlay = GraphOverlay::default();
        if let Some((title, x, y)) = self.graph_auto.take()
            && let Some(plot) = &mut self.plot
        {
            plot.title = title;
            plot.x_axis.label = x;
            plot.y_axis.label = y;
        }
        self.status = "Graph edits reset".to_owned();
    }

    /// Short explanation shown under a context-menu item.
    pub(super) fn graph_action_help(action: GraphAction) -> &'static str {
        match action {
            GraphAction::Labels => "Title, subtitle, axis labels and footer",
            GraphAction::Text => "Typeface and size of every text on the graph",
            GraphAction::DataDisplay => "Show or hide symbols, lines and bars",
            GraphAction::Annotation => "Place a free note inside the plot",
            GraphAction::Notes => "Duplicate, rotate, delete or reorder notes",
            GraphAction::DataLabels => "Label each point with its value or number",
            GraphAction::Reference => "Vertical or horizontal line with a label",
            GraphAction::Percentiles => "Lines at chosen percentiles of the points",
            GraphAction::PanelLayout => "Columns used by a multi-panel graph",
            GraphAction::AxisScale => "Automatic or manual X and Y limits",
            GraphAction::GridToggle => "Show or hide the grid",
            GraphAction::ReportToggle => "Show or hide the statistics block",
            GraphAction::Save => "Save the graph as SVG",
            GraphAction::SavePng => "Save a PNG image",
            GraphAction::SaveJpeg => "Save a JPEG image",
            GraphAction::Print => "Write a one-page PDF",
            GraphAction::Copy => "Copy the SVG to the clipboard",
            GraphAction::ClearMarks => "Remove the Ctrl+click marks",
            GraphAction::Reset => "Restore the automatic labels and remove the edits",
        }
    }

    /// Applies the graph labels while the dialog is open (live preview).
    fn preview_graph_labels(&mut self) {
        if let Some(plot) = &mut self.plot {
            plot.title = self.graph_label_title.trim().to_owned();
            plot.x_axis.label = self.graph_label_x.trim().to_owned();
            plot.y_axis.label = self.graph_label_y.trim().to_owned();
        }
        let subtitle = self.graph_label_subtitle.trim();
        self.graph_overlay.subtitle = (!subtitle.is_empty()).then(|| subtitle.to_owned());
        let footer = self.graph_label_footer.trim();
        self.graph_overlay.footer = (!footer.is_empty()).then(|| footer.to_owned());
    }

    /// Applies the typeface and size while the dialog is open (live preview).
    fn preview_graph_text(&mut self) {
        self.graph_overlay.font_family = self.graph_text_family;
        let size = if self.graph_text_size.is_finite() {
            self.graph_text_size.clamp(9.0, 18.0)
        } else {
            12.0
        };
        self.graph_overlay.font_scale = size / 12.0;
    }

    /// Applies whatever manual limits are valid while the dialog is open.
    fn preview_axis_scale(&mut self) {
        let x = Self::parse_range(&self.axis_x_min, &self.axis_x_max, "x");
        let y = Self::parse_range(&self.axis_y_min, &self.axis_y_max, "y");
        if let (Ok(x), Ok(y)) = (x, y) {
            self.graph_overlay.x_range = x;
            self.graph_overlay.y_range = y;
        }
    }

    /// Applies the panel columns while the dialog is open.
    fn preview_panel_layout(&mut self) {
        if let Some(plot) = &mut self.plot {
            plot.panel_columns = self.panel_columns_draft.clamp(1, 6);
        }
    }

    /// Runs an action chosen from the graph menu or its context menu.
    pub(super) fn apply_graph_action(&mut self, ctx: &egui::Context, action: GraphAction) {
        match action {
            GraphAction::Labels => self.open_graph_labels(),
            GraphAction::Text => self.open_graph_text(),
            GraphAction::Annotation => self.open_graph_annotation(),
            GraphAction::Notes => self.show_notes = true,
            GraphAction::DataLabels => self.open_data_labels(),
            GraphAction::Percentiles => self.open_percentiles(),
            GraphAction::PanelLayout => self.open_panel_layout(),
            GraphAction::AxisScale => self.open_axis_scale(),
            GraphAction::DataDisplay => {
                if self.plot.is_none() {
                    self.set_error("No graph to edit");
                } else {
                    self.show_data_display = true;
                }
            }
            GraphAction::ReportToggle => {
                self.graph_overlay.show_report = !self.graph_overlay.show_report;
                self.status = if self.graph_overlay.show_report {
                    "Report data shown".to_owned()
                } else {
                    "Report data hidden".to_owned()
                };
            }
            GraphAction::GridToggle => {
                self.graph_overlay.grid = !self.graph_overlay.grid;
                self.status = if self.graph_overlay.grid {
                    "Gridlines shown".to_owned()
                } else {
                    "Gridlines hidden".to_owned()
                };
            }
            GraphAction::Reference => self.open_reference_line(),
            GraphAction::Save => self.begin_plot_save(),
            GraphAction::SavePng => self.begin_graph_image(FileKind::Png),
            GraphAction::SaveJpeg => self.begin_graph_image(FileKind::Jpeg),
            GraphAction::Print => self.begin_graph_pdf(),
            GraphAction::Copy => {
                if let Some(plot) = &self.plot {
                    let svg = fp_core::plot::render_svg_with(plot, &self.graph_overlay);
                    ctx.copy_text(svg);
                    self.status = "Graph copied as SVG".to_owned();
                }
            }
            GraphAction::ClearMarks => {
                if let Ok(mut marks) = self.brush_marks.lock() {
                    marks.clear();
                }
                self.status = "Marks cleared".to_owned();
            }
            GraphAction::Reset => self.reset_graph_edits(),
        }
    }

    pub(super) fn graph_labels_window(&mut self, ctx: &egui::Context) {
        if !self.show_graph_labels {
            return;
        }
        let mut open = self.show_graph_labels;
        let mut apply = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(self.catalog().label("Graph options"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                for (label, field) in [
                    ("Title", &mut self.graph_label_title),
                    ("Subtitle", &mut self.graph_label_subtitle),
                    ("X axis", &mut self.graph_label_x),
                    ("Y axis", &mut self.graph_label_y),
                    ("Footer", &mut self.graph_label_footer),
                ] {
                    ui.horizontal(|ui| {
                        let help = ui.help_label(catalog.label(label));
                        ui.add(
                            egui::TextEdit::singleline(field)
                                .desired_width(320.0)
                                .hint_text(catalog.label("Empty hides the line")),
                        )
                        .labelled_by(help.id);
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_graph_labels = false;
                    }
                });
            });
        if apply {
            self.apply_graph_labels();
        } else {
            if open {
                self.preview_graph_labels();
            }
            self.show_graph_labels = open && self.show_graph_labels;
        }
    }

    pub(super) fn graph_annotation_window(&mut self, ctx: &egui::Context) {
        if !self.show_graph_annotation {
            return;
        }
        let mut open = self.show_graph_annotation;
        let mut add = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(self.catalog().label("Add note to the graph"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Text:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.annotation_text)
                            .desired_width(300.0)
                            .hint_text(catalog.label("Note to place on the graph")),
                    )
                    .labelled_by(label.id);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Position:"));
                    ui.add(
                        egui::DragValue::new(&mut self.annotation_x)
                            .range(0.0..=100.0)
                            .speed(0.5)
                            .suffix(" % X"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.annotation_y)
                            .range(0.0..=100.0)
                            .speed(0.5)
                            .suffix(" % Y"),
                    )
                    .labelled_by(label.id);
                });
                ui.small(catalog.label(
                    "0 % is the top-left corner of the plot and 100 % the bottom-right; the text is kept between sessions.",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Add")).clicked() {
                        add = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_graph_annotation = false;
                    }
                });
            });
        if add {
            self.add_graph_annotation();
        } else {
            self.show_graph_annotation = open && self.show_graph_annotation;
        }
    }

    pub(super) fn notes_window(&mut self, ctx: &egui::Context) {
        if !self.show_notes {
            return;
        }
        let mut open = self.show_notes;
        let mut duplicate = None;
        let mut delete = None;
        let mut rotate = None;
        let mut front = None;
        let catalog = self.catalog();
        let notes = self.graph_overlay.annotations.clone();
        self.graph_editor_dialog(catalog.label("Notes"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                if notes.is_empty() {
                    ui.weak(catalog.label("No notes yet."));
                }
                for (index, note) in notes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let text: String = note.text.chars().take(40).collect();
                        ui.label(format!("{}. {text}", index + 1));
                        if ui.help_button(catalog.label("Duplicate")).clicked() {
                            duplicate = Some(index);
                        }
                        if ui.help_button(catalog.label("Delete")).clicked() {
                            delete = Some(index);
                        }
                        if ui
                            .help_button("⟲")
                            .on_hover_text(catalog.label("Rotate left"))
                            .clicked()
                        {
                            rotate = Some((index, -15.0));
                        }
                        if ui
                            .help_button("⟳")
                            .on_hover_text(catalog.label("Rotate right"))
                            .clicked()
                        {
                            rotate = Some((index, 15.0));
                        }
                        if ui.help_button(catalog.label("To front")).clicked() {
                            front = Some(index);
                        }
                    });
                }
                ui.separator();
                if ui.help_button(catalog.label("Close")).clicked() {
                    self.show_notes = false;
                }
            });
        self.show_notes = open && self.show_notes;
        if let Some(index) = duplicate {
            self.duplicate_note(index);
        }
        if let Some(index) = delete {
            self.delete_note(index);
        }
        if let Some((index, delta)) = rotate {
            self.rotate_note(index, delta);
        }
        if let Some(index) = front {
            self.bring_note_to_front(index);
        }
    }

    pub(super) fn data_labels_window(&mut self, ctx: &egui::Context) {
        if !self.show_data_labels {
            return;
        }
        let mut open = self.show_data_labels;
        let mut apply = false;
        let mut remove = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(catalog.label("Data labels"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Label:"));
                    egui::ComboBox::from_id_salt("data_label_mode")
                        .selected_text(match self.data_label_mode {
                            DataLabelMode::YValue => catalog.label("Y value"),
                            DataLabelMode::Index => catalog.label("Point number"),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.data_label_mode,
                                DataLabelMode::YValue,
                                catalog.label("Y value"),
                            );
                            ui.selectable_value(
                                &mut self.data_label_mode,
                                DataLabelMode::Index,
                                catalog.label("Point number"),
                            );
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(catalog.label("One label per point, drawn next to the marker."));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.help_button(catalog.label("Remove labels")).clicked() {
                        remove = true;
                    }
                    if ui.help_button(catalog.label("Cancel")).clicked() {
                        self.show_data_labels = false;
                    }
                });
            });
        if apply {
            self.apply_data_labels(false);
        } else if remove {
            self.apply_data_labels(true);
        } else {
            if open {
                self.graph_overlay.data_labels = Some(self.data_label_mode);
            }
            self.show_data_labels = open && self.show_data_labels;
        }
    }

    pub(super) fn percentiles_window(&mut self, ctx: &egui::Context) {
        if !self.show_percentiles {
            return;
        }
        let mut open = self.show_percentiles;
        let mut add = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(catalog.label("Percentile lines"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.percentile_axis,
                        ReferenceAxis::Vertical,
                        catalog.label("X values"),
                    );
                    ui.selectable_value(
                        &mut self.percentile_axis,
                        ReferenceAxis::Horizontal,
                        catalog.label("Y values"),
                    );
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Percentiles:"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.percentile_values)
                            .desired_width(260.0),
                    )
                    .labelled_by(label.id);
                });
                ui.small(catalog.label(
                    "Comma-separated percentages between 0 and 100; each adds a labelled reference line.",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Add")).clicked() {
                        add = true;
                    }
                    if ui.help_button(catalog.label("Cancel")).clicked() {
                        self.show_percentiles = false;
                    }
                });
            });
        if add {
            self.add_percentile_lines();
        } else {
            self.show_percentiles = open && self.show_percentiles;
        }
    }

    pub(super) fn panel_layout_window(&mut self, ctx: &egui::Context) {
        if !self.show_panel_layout {
            return;
        }
        let mut open = self.show_panel_layout;
        let mut apply = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(catalog.label("Panel layout"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Columns:"));
                    ui.add(egui::DragValue::new(&mut self.panel_columns_draft).range(1..=6))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(catalog.label("Panels are filled left to right, top to bottom."));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.help_button(catalog.label("Cancel")).clicked() {
                        self.show_panel_layout = false;
                    }
                });
            });
        if apply {
            self.apply_panel_layout();
        } else {
            if open {
                self.preview_panel_layout();
            }
            self.show_panel_layout = open && self.show_panel_layout;
        }
    }

    pub(super) fn axis_scale_window(&mut self, ctx: &egui::Context) {
        if !self.show_axis_scale {
            return;
        }
        let mut open = self.show_axis_scale;
        let mut apply = false;
        let mut automatic = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(catalog.label("Axis scale"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                for (title, low, high) in [
                    ("X axis", &mut self.axis_x_min, &mut self.axis_x_max),
                    ("Y axis", &mut self.axis_y_min, &mut self.axis_y_max),
                ] {
                    ui.horizontal(|ui| {
                        ui.help_label(catalog.label(title));
                        ui.label(catalog.label("from"));
                        ui.add(egui::TextEdit::singleline(low).desired_width(90.0))
                            .labelled_by(ui.help_label(catalog.label("Minimum")).id);
                        ui.label(catalog.label("to"));
                        ui.add(egui::TextEdit::singleline(high).desired_width(90.0))
                            .labelled_by(ui.help_label(catalog.label("Maximum")).id);
                    });
                }
                ui.small(catalog.label(
                    "Leave both fields of an axis empty for automatic limits; probabilities and panels keep their own scales.",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(catalog.label("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.help_button(catalog.label("Automatic")).clicked() {
                        automatic = true;
                    }
                    if ui.help_button(catalog.label("Cancel")).clicked() {
                        self.show_axis_scale = false;
                    }
                });
            });
        if apply {
            self.apply_axis_scale(false);
        } else if automatic {
            self.apply_axis_scale(true);
        } else {
            if open {
                self.preview_axis_scale();
            }
            self.show_axis_scale = open && self.show_axis_scale;
        }
    }

    pub(super) fn graph_text_window(&mut self, ctx: &egui::Context) {
        if !self.show_graph_text {
            return;
        }
        let mut open = self.show_graph_text;
        let mut apply = false;
        let mut reset = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(self.catalog().label("Graph text"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Typeface:"));
                    egui::ComboBox::from_id_salt("graph_text_family")
                        .selected_text(match self.graph_text_family {
                            fp_core::plot::GraphFontFamily::Sans => catalog.label("Sans"),
                            fp_core::plot::GraphFontFamily::Serif => catalog.label("Serif"),
                            fp_core::plot::GraphFontFamily::Mono => catalog.label("Monospace"),
                        })
                        .show_ui(ui, |ui| {
                            for (family, name) in [
                                (fp_core::plot::GraphFontFamily::Sans, "Sans"),
                                (fp_core::plot::GraphFontFamily::Serif, "Serif"),
                                (fp_core::plot::GraphFontFamily::Mono, "Monospace"),
                            ] {
                                ui.selectable_value(
                                    &mut self.graph_text_family,
                                    family,
                                    catalog.label(name),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Text size:"));
                    if ui
                        .help_button(catalog.label("A−"))
                        .on_hover_text(catalog.label("Smaller text"))
                        .clicked()
                    {
                        self.graph_text_size = (self.graph_text_size - 1.0).max(9.0);
                    }
                    ui.add(
                        egui::DragValue::new(&mut self.graph_text_size)
                            .range(9.0..=18.0)
                            .speed(0.25)
                            .suffix(" pt"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    if ui
                        .help_button(catalog.label("A+"))
                        .on_hover_text(catalog.label("Larger text"))
                        .clicked()
                    {
                        self.graph_text_size = (self.graph_text_size + 1.0).min(18.0);
                    }
                });
                ui.small(catalog.label(
                    "The size and typeface apply to every text drawn on the graph; the settings are kept for the next session.",
                ));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Apply")).clicked() {
                        apply = true;
                    }
                    if ui.help_button(self.catalog().label("Reset")).clicked() {
                        reset = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_graph_text = false;
                    }
                });
            });
        if reset {
            self.graph_text_family = fp_core::plot::GraphFontFamily::Sans;
            self.graph_text_size = 12.0;
        } else if apply {
            self.apply_graph_text();
        } else {
            if open {
                self.preview_graph_text();
            }
            self.show_graph_text = open && self.show_graph_text;
        }
    }

    pub(super) fn reference_line_window(&mut self, ctx: &egui::Context) {
        if !self.show_reference_line {
            return;
        }
        let mut open = self.show_reference_line;
        let mut add = false;
        let catalog = self.catalog();
        self.graph_editor_dialog(self.catalog().label("Reference line"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.reference_axis,
                        ReferenceAxis::Vertical,
                        catalog.label("Vertical (x = value)"),
                    );
                    ui.selectable_value(
                        &mut self.reference_axis,
                        ReferenceAxis::Horizontal,
                        catalog.label("Horizontal (y = value)"),
                    );
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Value"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.reference_value).desired_width(120.0),
                    )
                    .labelled_by(label.id);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label(catalog.label("Label"));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.reference_label).desired_width(220.0),
                    )
                    .labelled_by(label.id);
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Add")).clicked() {
                        add = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_reference_line = false;
                    }
                });
            });
        if add {
            self.add_reference_line();
        } else {
            self.show_reference_line = open && self.show_reference_line;
        }
    }
}
