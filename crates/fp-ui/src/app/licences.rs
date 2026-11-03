//! Third-party notices, shown from the Help menu and shipped with the binary.

use super::*;

/// Every notice that must accompany a copy of the binary: the project's MIT
/// licence, the dependency summary and the DejaVu Sans font licence.
pub const THIRD_PARTY_NOTICES: &str = include_str!("../../assets/THIRD-PARTY-NOTICES.txt");

impl FreePlotsApp {
    pub(super) fn licences_window(&mut self, ctx: &egui::Context) {
        if !self.show_licences {
            return;
        }
        let mut open = self.show_licences;
        dialog_window(self.catalog().label("Third-party licences"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(460.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(THIRD_PARTY_NOTICES)
                                    .monospace()
                                    .size(11.0),
                            )
                            .wrap(),
                        );
                    });
                ui.separator();
                ui.weak(self.catalog().label(
                    "These notices must accompany every copy of the program; they also ship as LICENSES.txt next to the executable.",
                ));
            });
        self.show_licences = open;
    }
}
