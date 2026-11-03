//! Graph presentation edits: labels, subtitle, footer and reference lines.

use super::*;
use fp_core::plot::{GraphOverlay, ReferenceAxis, ReferenceLine};

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
