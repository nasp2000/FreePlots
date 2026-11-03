//! Session panel, report text and the undo/redo history.

use super::*;

impl FreePlotsApp {
    pub(super) fn push_undo(&mut self, group: EditGroup) {
        if group.actions.is_empty() {
            return;
        }
        if self.undo_stack.len() == UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(group);
        self.redo_stack.clear();
    }
    pub fn undo(&mut self) {
        let Some(group) = self.undo_stack.pop() else {
            self.status = "Nothing to undo".to_owned();
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        for action in group.actions.iter().rev() {
            if let Err(e) = undo_action(df, action) {
                self.set_error(e);
                return;
            }
        }
        self.redo_stack.push(group.clone());
        self.error = None;
        self.status = format!("Undo — {} change(s)", group.actions.len());
        self.restore_after_history();
    }
    pub fn redo(&mut self) {
        let Some(group) = self.redo_stack.pop() else {
            self.status = "Nothing to redo".to_owned();
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        for action in &group.actions {
            if let Err(e) = redo_action(df, action) {
                self.set_error(e);
                return;
            }
        }
        self.undo_stack.push(group.clone());
        self.error = None;
        self.status = format!("Redo — {} change(s)", group.actions.len());
        self.restore_after_history();
    }
    pub(super) fn restore_after_history(&mut self) {
        let target = self
            .undo_stack
            .last()
            .or_else(|| self.redo_stack.last())
            .and_then(group_target);
        if let Some((column, row)) = target {
            self.select_cell(column, row);
        } else {
            self.refresh_column_summary();
        }
    }
    /// `true` when a worksheet cell edit can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    /// `true` when an undone edit can be re-applied.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
    pub fn begin_session_export(&mut self) {
        self.save_kind = FileKind::Html;
        self.save_path = "session.html".to_owned();
        self.show_save = true;
    }
    pub fn mark_session_exported(&mut self, path: &str) {
        self.status = format!("Session exported to {path}");
        self.error = None;
    }
    /// Session text, for tests and the desktop runner.
    #[must_use]
    pub fn session_text(&self) -> &str {
        &self.session
    }
    pub(super) fn append_result(&mut self, result: &fp_core::report::AnalysisResult) {
        let text = result.to_session_text_lang(fp_core::i18n::Catalog::new(self.language));
        self.append_session(&text);
    }
    pub(super) fn append_session(&mut self, text: &str) {
        if !self.session.is_empty() {
            self.session.push_str("\n\n");
        }
        self.session.push_str(text);
        // A long session is trimmed from the front, on a report boundary, so
        // the window stays responsive on machines with little memory.
        if self.session.len() > SESSION_LIMIT {
            let cut = self.session.len() - SESSION_LIMIT;
            let start = self
                .session
                .char_indices()
                .find(|(index, ch)| *index >= cut && ch.is_ascii_whitespace())
                .map_or(0, |(index, _)| index + 1);
            self.session.drain(..start);
        }
    }
    pub(super) fn session_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Session");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Clear")
                    .on_hover_text("Clear the Session output")
                    .clicked()
                {
                    self.session.clear();
                }
            });
        });
        ui.separator();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.session.is_empty() {
                    ui.label(
                        RichText::new("Analysis output appears here.")
                            .italics()
                            .color(ui.visuals().text_color()),
                    );
                } else {
                    ui.add(
                        egui::Label::new(RichText::new(&self.session).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend),
                    )
                    .on_hover_text("Analysis reports appear here");
                }
            });
    }
}
