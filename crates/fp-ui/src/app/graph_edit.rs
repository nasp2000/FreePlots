//! Graph presentation edits: labels, subtitle, footer and reference lines.

use super::*;
use fp_core::plot::{Annotation, GraphOverlay, ReferenceAxis, ReferenceLine};

/// Action chosen from the graph context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphAction {
    Labels,
    Text,
    Annotation,
    Reference,
    Save,
    Copy,
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
        self.show_graph_annotation = true;
    }

    /// Places the annotation at the chosen fraction of the plot area.
    pub fn add_graph_annotation(&mut self) {
        let text = self.annotation_text.trim();
        if text.is_empty() {
            self.set_error("Type the text to place on the graph");
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
        });
        self.show_graph_annotation = false;
        self.error = None;
        self.status = "Text added to the graph".to_owned();
    }

    /// Applies the typeface and size to every text drawn on the graph.
    pub fn apply_graph_text(&mut self) {
        self.graph_overlay.font_family = self.graph_text_family;
        let size = if self.graph_text_size.is_finite() {
            self.graph_text_size.clamp(9.0, 18.0)
        } else {
            12.0
        };
        self.graph_overlay.font_scale = size / 12.0;
        self.show_graph_text = false;
        self.status = "Graph text updated".to_owned();
    }

    /// Applies the label editor.
    pub fn apply_graph_labels(&mut self) {
        if let Some(plot) = &mut self.plot {
            plot.title = self.graph_label_title.trim().to_owned();
            plot.x_axis.label = self.graph_label_x.trim().to_owned();
            plot.y_axis.label = self.graph_label_y.trim().to_owned();
        }
        let subtitle = self.graph_label_subtitle.trim();
        self.graph_overlay.subtitle = (!subtitle.is_empty()).then(|| subtitle.to_owned());
        let footer = self.graph_label_footer.trim();
        self.graph_overlay.footer = (!footer.is_empty()).then(|| footer.to_owned());
        self.show_graph_labels = false;
        self.status = "Graph labels updated".to_owned();
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

    /// Runs an action chosen from the graph menu or its context menu.
    pub(super) fn apply_graph_action(&mut self, ctx: &egui::Context, action: GraphAction) {
        match action {
            GraphAction::Labels => self.open_graph_labels(),
            GraphAction::Text => self.open_graph_text(),
            GraphAction::Annotation => self.open_graph_annotation(),
            GraphAction::Reference => self.open_reference_line(),
            GraphAction::Save => self.begin_plot_save(),
            GraphAction::Copy => {
                if let Some(plot) = &self.plot {
                    let svg = fp_core::plot::render_svg_with(plot, &self.graph_overlay);
                    ctx.copy_text(svg);
                    self.status = "Graph copied as SVG".to_owned();
                }
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
        dialog_window(self.catalog().label("Graph labels"))
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
        dialog_window(self.catalog().label("Add text to the graph"))
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
                            .hint_text(catalog.label("Text to place on the graph")),
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

    pub(super) fn graph_text_window(&mut self, ctx: &egui::Context) {
        if !self.show_graph_text {
            return;
        }
        let mut open = self.show_graph_text;
        let mut apply = false;
        let mut reset = false;
        let catalog = self.catalog();
        dialog_window(self.catalog().label("Graph text"))
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
                    ui.add(
                        egui::DragValue::new(&mut self.graph_text_size)
                            .range(9.0..=18.0)
                            .speed(0.25)
                            .suffix(" pt"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
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
        dialog_window(self.catalog().label("Reference line"))
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
