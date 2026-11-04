//! Worksheet grid: headers, cells, inline editor, selection and keyboard.

use super::*;

impl FreePlotsApp {
    pub(super) fn grid_cell(ui: &egui::Ui, color: Color32) {
        let rect = ui.max_rect();
        let stroke = egui::Stroke::new(1.0, color);
        ui.painter().line_segment(
            [
                egui::pos2(rect.right() - 0.5, rect.top()),
                egui::pos2(rect.right() - 0.5, rect.bottom()),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), rect.bottom() - 0.5),
                egui::pos2(rect.right(), rect.bottom() - 0.5),
            ],
            stroke,
        );
    }

    pub(super) fn worksheet(&mut self, ui: &mut egui::Ui) {
        if self.data.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("No worksheet open.\nFile → Open CSV…")
                        .size(16.0)
                        .color(ui.visuals().text_color()),
                );
            });
            return;
        }

        let (cell_col, cell_row) = self.selected_cell.unwrap_or((0, 0));
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.strong("fx")
                .on_hover_text("Formula bar: type a value or an expression for the selected cell");
            ui.help_label(format!("C{} R{}", cell_col + 1, cell_row + 1))
                .on_hover_text(format!(
                    "Selected cell: column C{}, row {}. Type in the field, then press Enter or \
                     the tick to write it here.",
                    cell_col + 1,
                    cell_row + 1
                ));
            let response = ui
                .add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .desired_width((ui.available_width() - 40.0).max(80.0)),
                )
                .on_hover_text("Type a value or =SUM(C1R1:C1R10); Enter commits the result. References are C<column>R<row>");
            self.formula_focused = response.has_focus();
            if response.changed() {
                self.edit_dirty = true;
            }
            if response.lost_focus()
                && (self.edit_dirty || ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                commit = true;
            }
            if apply_icon_button(ui).clicked() {
                commit = true;
            }
        });
        if commit {
            self.commit_cell();
        }
        ui.separator();

        let Some(df) = self.data.as_ref() else {
            return;
        };
        let names: Vec<String> = df.columns().iter().map(|c| c.name().to_owned()).collect();
        let n_rows = df.n_rows();
        // classic worksheets always show spare columns and rows; painting a few blank ones
        // keeps the worksheet a grid instead of a small island of data.
        let display_cols = names.len().max(26);
        let display_rows = n_rows.max(60);
        let selected_cell = self.selected_cell;
        let selection = self.selection_bounds();
        let selection_size = self.selection_size();
        let shift = ui.input(|i| i.modifiers.shift);
        let mut clicked: Option<(usize, usize, bool)> = None;
        let mut clicked_header_column: Option<usize> = None;
        let mut clicked_header_row: Option<usize> = None;
        let mut select_all = false;
        let mut menu_action: Option<GridMenu> = None;
        let grid_lines = self.show_grid_lines;
        let dark = self.dark_mode;
        let cell_bg = if dark {
            Color32::from_rgb(20, 23, 27)
        } else {
            Color32::from_rgb(252, 250, 246)
        };
        let cell_fg = if dark {
            Color32::from_rgb(230, 236, 239)
        } else {
            Color32::from_rgb(34, 39, 43)
        };
        let header_bg = if dark {
            Color32::from_rgb(29, 34, 41)
        } else {
            Color32::from_rgb(244, 241, 234)
        };
        let line = if dark {
            Color32::from_rgb(44, 52, 61)
        } else {
            Color32::from_rgb(218, 212, 201)
        };
        let header_line = if dark {
            Color32::from_rgb(58, 68, 78)
        } else {
            Color32::from_rgb(208, 201, 189)
        };
        let selection_bg = if dark {
            Color32::from_rgb(28, 72, 68)
        } else {
            Color32::from_rgb(223, 242, 237)
        };
        let selected_bg = if dark {
            Color32::from_rgb(36, 92, 86)
        } else {
            Color32::from_rgb(205, 238, 230)
        };
        let row_hl = if dark {
            Color32::from_rgb(35, 41, 49)
        } else {
            Color32::from_rgb(238, 234, 225)
        };
        let muted = if dark {
            Color32::from_rgb(230, 236, 239)
        } else {
            Color32::from_rgb(34, 39, 43)
        };
        let hint = ui.visuals().text_color();
        let column_name = ui.visuals().text_color();
        let inline_cell = self.inline_edit;
        let mut inline_buffer = std::mem::take(&mut self.inline_buffer);
        let mut inline_focus = self.inline_request_focus;
        let mut inline_commit_cell: Option<(usize, usize)> = None;
        let mut inline_cancel = false;
        let mut inline_nav: Option<(isize, isize)> = None;
        let mut inline_start: Option<(usize, usize)> = None;

        ui.horizontal(|ui| {
            ui.strong("Worksheet");
            ui.label(
                RichText::new(format!("{} × {} columns", rows_label(n_rows), names.len()))
                    .size(13.5)
                    .color(hint),
            );
            if selection_size.0 * selection_size.1 > 1 {
                ui.label(
                    RichText::new(format!(
                        "{} × {} selected",
                        selection_size.0, selection_size.1
                    ))
                    .size(13.5)
                    .color(hint),
                );
            }
        });
        ui.separator();

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let mut new_widths: Option<Vec<f32>> = None;
        egui::ScrollArea::horizontal()
            .id_salt("fp_grid_h")
            .auto_shrink([false, true])
            .show_viewport(ui, |ui, viewport| {
                let widths: Vec<f32> = (0..display_cols)
                    .map(|index| self.column_widths.get(index).copied().unwrap_or(72.0))
                    .collect();
                let total: f32 = widths.iter().sum();
                ui.set_min_width(44.0 + total);
                // Only the columns inside the viewport (plus a margin) are laid
                // out, so a thousand-column sheet costs what a screenful
                // costs. The columns left of the viewport become one spacer
                // column, which keeps the scroll width and the alignment.
                let mut first = 0usize;
                let mut prefix = 0.0;
                while first + 1 < display_cols && prefix + widths[first] < viewport.min.x - 144.0 {
                    prefix += widths[first];
                    first += 1;
                }
                let mut last = first;
                let mut visible_width = 0.0;
                while last < display_cols
                    && (last == first || visible_width < viewport.width() + 288.0)
                {
                    visible_width += widths[last];
                    last += 1;
                }
                let visible = first..last;
                let mut measured = widths.clone();
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(Align::Center))
                    .column(TableColumn::exact(44.0));
                if first > 0 {
                    table = table.column(TableColumn::exact(prefix).resizable(false));
                }
                for width in &widths[visible.clone()] {
                    table =
                        table.column(TableColumn::initial(*width).at_least(48.0).resizable(true));
                }
                table
                    .header(36.0, |mut header| {
                        header.col(|ui| {
                            ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                            let response = ui
                                .add(
                                    egui::Label::new(RichText::new("#").strong())
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_text("Select the whole worksheet");
                            if response.clicked() {
                                select_all = true;
                            }
                            if grid_lines {
                                Self::grid_cell(ui, header_line);
                            }
                        });
                        if first > 0 {
                            header.col(|ui| {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                            });
                        }
                        for index in visible.clone() {
                            header.col(|ui| {
                                measured[index] = ui.max_rect().width();
                                ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                                if let Some(name) = names.get(index) {
                                    ui.vertical(|ui| {
                                        let is_selected = selected_cell
                                            .is_some_and(|(selected, _)| selected == index)
                                            || selection.is_some_and(|(c0, _, c1, _)| {
                                                index >= c0 && index <= c1
                                            });
                                        let header = ui
                                            .selectable_label(
                                                is_selected,
                                                RichText::new(format!("C{}", index + 1)).strong(),
                                            )
                                            .on_hover_text(format!(
                                                "Column C{}: {} — click to select the column",
                                                index + 1,
                                                name
                                            ));
                                        if header.clicked() {
                                            clicked_header_column = Some(index);
                                        }
                                        if header.secondary_clicked() {
                                            clicked_header_column = Some(index);
                                        }
                                        header.context_menu(|ui| {
                                            if ui
                                                .button("Add numeric column")
                                                .on_hover_text(button_help("Add numeric column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Add text column")
                                                .on_hover_text(button_help("Add text column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddTextColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Add text column")
                                                .on_hover_text(button_help("Add text column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddTextColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete column")
                                                .on_hover_text(button_help("Delete column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Rename column…")
                                                .on_hover_text(button_help("Rename column…"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::RenameColumn);
                                                ui.close();
                                            }
                                        });
                                        if name != &format!("C{}", index + 1) {
                                            ui.label(
                                                RichText::new(name).small().color(column_name),
                                            );
                                        }
                                    });
                                } else {
                                    ui.strong(format!("C{}", index + 1));
                                }
                                if grid_lines {
                                    Self::grid_cell(ui, header_line);
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(20.0, display_rows, |mut row| {
                            let r = row.index();
                            row.col(|ui| {
                                let in_selection =
                                    selection.is_some_and(|(_, r0, _, r1)| r >= r0 && r <= r1);
                                ui.painter().rect_filled(
                                    ui.max_rect(),
                                    0.0,
                                    if in_selection { row_hl } else { cell_bg },
                                );
                                // The whole cell is the target, not just the digits.
                                let rect = ui.max_rect();
                                let response = ui
                                    .interact(
                                        rect,
                                        egui::Id::new(("fp_row", r)),
                                        egui::Sense::click(),
                                    )
                                    .on_hover_text("Select the whole row");
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    (r + 1).to_string(),
                                    egui::FontId::monospace(12.0),
                                    muted,
                                );
                                if response.clicked() {
                                    clicked_header_row = Some(r);
                                }
                                if response.secondary_clicked() {
                                    clicked_header_row = Some(r);
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .button("Insert row above")
                                        .on_hover_text(button_help("Insert row above"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::InsertRowAbove);
                                        ui.close();
                                    }
                                    if ui
                                        .button("Insert row below")
                                        .on_hover_text(button_help("Insert row below"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::InsertRowBelow);
                                        ui.close();
                                    }
                                    if ui
                                        .button("Delete row")
                                        .on_hover_text(button_help("Delete row"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::DeleteRow);
                                        ui.close();
                                    }
                                });
                                if grid_lines {
                                    Self::grid_cell(ui, line);
                                }
                            });
                            if first > 0 {
                                row.col(|ui| {
                                    let rect = ui.max_rect();
                                    ui.painter()
                                        .with_clip_rect(rect)
                                        .rect_filled(rect, 0.0, cell_bg);
                                });
                            }
                            for index in visible.clone() {
                                row.col(|ui| {
                                    // Paint and hit-test the whole cell, so a
                                    // click anywhere selects it (an empty cell
                                    // has no text to click on).
                                    let rect = ui.max_rect();
                                    let painter = ui.painter().with_clip_rect(rect);
                                    painter.rect_filled(rect, 0.0, cell_bg);
                                    let is_selected = selected_cell == Some((index, r));
                                    let in_selection = selection.is_some_and(|(c0, r0, c1, r1)| {
                                        index >= c0 && index <= c1 && r >= r0 && r <= r1
                                    });
                                    // Conditional formatting sits under the
                                    // selection so the selected cell stays
                                    // visible, and follows the column name.
                                    let rule = (index < names.len())
                                        .then(|| self.conditional_style(df, index, r))
                                        .flatten();
                                    if let Some(style) = rule
                                        && let Some(color) = conditional_fill(style, dark)
                                    {
                                        painter.rect_filled(rect, 0.0, color);
                                    }
                                    if in_selection {
                                        painter.rect_filled(rect, 0.0, selection_bg);
                                    }
                                    if is_selected {
                                        painter.rect_filled(rect, 0.0, selected_bg);
                                    }
                                    let text = if r < n_rows && index < names.len() {
                                        self.display_cell(df, index, r)
                                    } else {
                                        String::new()
                                    };
                                    let format = names
                                        .get(index)
                                        .and_then(|name| self.formats.get(name))
                                        .copied();
                                    let text = format_cell_text(text, format);
                                    if inline_cell == Some((index, r)) {
                                        let response = ui.add(
                                            egui::TextEdit::singleline(&mut inline_buffer)
                                                .id(egui::Id::new(("fp_inline", index, r)))
                                                .desired_width(ui.available_width()),
                                        );
                                        if inline_focus {
                                            response.request_focus();
                                            inline_focus = false;
                                        }
                                        // Enter, Tab and the arrow keys commit the
                                        // edit and move the selection, as in a
                                        // worksheet: the formula bar stays the
                                        // place for editing text with the caret.
                                        let nav = ui.input(|i| {
                                            if i.key_pressed(egui::Key::Enter)
                                                || i.key_pressed(egui::Key::ArrowDown)
                                            {
                                                Some((0, 1))
                                            } else if i.key_pressed(egui::Key::ArrowUp) {
                                                Some((0, -1))
                                            } else if i.key_pressed(egui::Key::ArrowRight) {
                                                Some((1, 0))
                                            } else if i.key_pressed(egui::Key::ArrowLeft) {
                                                Some((-1, 0))
                                            } else if i.key_pressed(egui::Key::Tab) {
                                                if i.modifiers.shift {
                                                    Some((-1, 0))
                                                } else {
                                                    Some((1, 0))
                                                }
                                            } else {
                                                None
                                            }
                                        });
                                        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                        if escape && (response.has_focus() || response.lost_focus())
                                        {
                                            inline_cancel = true;
                                        } else if let Some((columns, rows)) = nav.filter(|_| {
                                            response.has_focus() || response.lost_focus()
                                        }) {
                                            response.surrender_focus();
                                            inline_commit_cell = Some((index, r));
                                            inline_nav = Some((columns, rows));
                                        } else if response.lost_focus() {
                                            inline_commit_cell = Some((index, r));
                                        }
                                    } else {
                                        let response = ui
                                            .interact(
                                                rect,
                                                egui::Id::new(("fp_cell", index, r)),
                                                egui::Sense::click(),
                                            )
                                            .on_hover_text(format!(
                                                "C{} R{} — double-click to edit",
                                                index + 1,
                                                r + 1
                                            ));
                                        let (anchor, align) = match format
                                            .map_or(CellAlign::Left, |format| format.align)
                                        {
                                            CellAlign::Left => (
                                                egui::pos2(rect.left() + 6.0, rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                            ),
                                            CellAlign::Center => (
                                                egui::pos2(rect.center().x, rect.center().y),
                                                egui::Align2::CENTER_CENTER,
                                            ),
                                            CellAlign::Right => (
                                                egui::pos2(rect.right() - 6.0, rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                            ),
                                        };
                                        let font = egui::FontId::monospace(12.0);
                                        let text_color = rule
                                            .and_then(|style| conditional_text(style, dark))
                                            .unwrap_or(cell_fg);
                                        painter.text(
                                            anchor,
                                            align,
                                            &text,
                                            font.clone(),
                                            text_color,
                                        );
                                        if format.is_some_and(|format| format.bold)
                                            || rule == Some(ConditionalStyle::Bold)
                                        {
                                            // egui has a single weight per family; drawing the
                                            // text twice fakes a bold face.
                                            painter.text(
                                                anchor + egui::vec2(0.6, 0.0),
                                                align,
                                                &text,
                                                font,
                                                cell_fg,
                                            );
                                        }
                                        // Screen readers get the cell address and the
                                        // text as painted (formatting included).
                                        let cell_label = if text.is_empty() {
                                            format!("C{} R{}", index + 1, r + 1)
                                        } else {
                                            format!("C{} R{}: {text}", index + 1, r + 1)
                                        };
                                        response.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Label,
                                                true,
                                                &cell_label,
                                            )
                                        });
                                        if response.clicked() && index < names.len() {
                                            clicked = Some((index, r, shift));
                                        }
                                        if response.double_clicked() && index < names.len() {
                                            inline_start = Some((index, r));
                                        }
                                        if response.secondary_clicked()
                                            && index < names.len()
                                            && !in_selection
                                        {
                                            // Right-clicking inside the selection keeps it.
                                            clicked = Some((index, r, false));
                                        }
                                        response.context_menu(|ui| {
                                            if ui
                                                .button("Cut")
                                                .on_hover_text(button_help("Cut"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Cut);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Copy")
                                                .on_hover_text(button_help("Copy"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Copy);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Paste")
                                                .on_hover_text(button_help("Paste"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Paste);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Clear")
                                                .on_hover_text(button_help("Clear"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Clear);
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui
                                                .button("Insert row above")
                                                .on_hover_text(button_help("Insert row above"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::InsertRowAbove);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Insert row below")
                                                .on_hover_text(button_help("Insert row below"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::InsertRowBelow);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete row")
                                                .on_hover_text(button_help("Delete row"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteRow);
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui
                                                .button("Add numeric column")
                                                .on_hover_text(button_help("Add numeric column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete column")
                                                .on_hover_text(button_help("Delete column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Rename column…")
                                                .on_hover_text(button_help("Rename column…"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::RenameColumn);
                                                ui.close();
                                            }
                                        });
                                    }
                                    if grid_lines {
                                        Self::grid_cell(ui, line);
                                    }
                                });
                            }
                        });
                    });
                if measured.iter().all(|width| *width > 0.0) {
                    new_widths = Some(measured.clone());
                }
            });
        if let Some(widths) = new_widths {
            self.column_widths = widths;
        }

        self.inline_buffer = inline_buffer;
        self.inline_request_focus = inline_focus;
        if let Some(action) = menu_action {
            match action {
                GridMenu::Cut => {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                    self.clear_selection();
                }
                GridMenu::Copy => {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                }
                GridMenu::Paste => {
                    if self.clipboard.is_empty() {
                        self.paste_request = true;
                    } else {
                        self.paste_internal();
                    }
                }
                GridMenu::Clear => self.clear_selection(),
                GridMenu::InsertRowAbove => self.insert_row(),
                GridMenu::InsertRowBelow => self.insert_row_below(),
                GridMenu::DeleteRow => self.delete_row(),
                GridMenu::AddColumn => self.add_column(),
                GridMenu::AddTextColumn => self.add_text_column(),
                GridMenu::DeleteColumn => self.delete_column(),
                GridMenu::RenameColumn => self.begin_rename_column(),
            }
        }
        if select_all {
            self.select_all_cells();
        }
        if let Some(column) = clicked_header_column {
            self.select_column(column);
        }
        if let Some(row) = clicked_header_row {
            self.select_row(row);
        }
        if inline_cancel {
            self.cancel_inline_edit();
        }
        let mut moved_from_editor = false;
        if let Some(cell) = inline_commit_cell {
            self.inline_edit = Some(cell);
            if !self.commit_inline_edit() {
                return;
            }
            if let Some((columns, rows)) = inline_nav {
                self.move_selection(columns, rows);
                // The same key event must not move the selection a second
                // time in the sheet-wide handler below.
                moved_from_editor = true;
            }
        }
        if let Some((column, row)) = inline_start {
            self.begin_inline_edit(column, row);
            return;
        }
        if let Some((column, row, extend)) = clicked {
            if self.edit_dirty && !self.commit_cell() {
                return;
            }
            if extend {
                self.extend_selection(column, row);
            } else {
                self.select_cell(column, row);
            }
        }
        if !moved_from_editor {
            self.handle_sheet_keys(ui);
        }
    }

    pub(super) fn handle_sheet_keys(&mut self, ui: &mut egui::Ui) {
        // Never steal the keystrokes of a focused text field (the formula bar,
        // an inline editor or any dialog field).
        if self.formula_focused
            || self.inline_edit.is_some()
            || ui.ctx().egui_wants_keyboard_input()
        {
            return;
        }
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let columns = df.n_columns();
        let rows = df.n_rows().max(60);
        let mut next = (column, row);
        let mut clear = false;
        let mut copy = false;
        let mut cut = false;
        let mut paste: Option<String> = None;
        let mut undo = false;
        let mut redo = false;
        let mut extend = false;
        let mut fill_down = false;
        let mut fill_right = false;
        let mut typed: Option<String> = None;
        ui.input(|input| {
            let shift = input.modifiers.shift;
            if shift
                && (input.key_pressed(egui::Key::ArrowLeft)
                    || input.key_pressed(egui::Key::ArrowRight)
                    || input.key_pressed(egui::Key::ArrowUp)
                    || input.key_pressed(egui::Key::ArrowDown))
            {
                extend = true;
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                next.0 = next.0.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                next.0 = (next.0 + 1).min(columns.saturating_sub(1));
            }
            if input.key_pressed(egui::Key::ArrowUp) {
                next.1 = next.1.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::ArrowDown) || input.key_pressed(egui::Key::Enter) {
                next.1 += 1;
            }
            if input.key_pressed(egui::Key::Tab) {
                next.0 = (next.0 + 1).min(columns.saturating_sub(1));
            }
            if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
                clear = true;
            }
            // egui-winit turns Ctrl+C/X/V into Copy/Cut/Paste events and
            // swallows the key events, so the clipboard has to be read from
            // the events themselves.
            for event in &input.events {
                match event {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => {
                        copy = true;
                        cut = true;
                    }
                    egui::Event::Paste(text) => paste = Some(text.clone()),
                    _ => {}
                }
            }
            if input.modifiers.command && input.key_pressed(egui::Key::C) {
                copy = true;
            }
            if input.modifiers.command
                && (input.key_pressed(egui::Key::Y)
                    || (input.modifiers.shift && input.key_pressed(egui::Key::Z)))
            {
                redo = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::Z) && !input.modifiers.shift
            {
                undo = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::D) {
                fill_down = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::R) {
                fill_right = true;
            }

            // Excel behaviour: typing over a selected cell starts the
            // edit with that character instead of requiring a double-click.
            if !input.modifiers.command && !input.modifiers.alt {
                for event in &input.events {
                    if let egui::Event::Text(text) = event
                        && !text.is_empty()
                        && text.chars().all(|c| !c.is_control())
                    {
                        typed = Some(text.clone());
                    }
                }
            }
        });
        if fill_down {
            self.fill_down();
            return;
        }
        if fill_right {
            self.fill_right();
            return;
        }
        if undo {
            self.undo();
            return;
        }
        if redo {
            self.redo();
            return;
        }
        if copy && let Some(text) = self.selection_tsv() {
            self.clipboard = text.clone();
            ui.ctx().copy_text(text);
        }
        if cut {
            self.clear_selection();
        }
        if let Some(text) = typed {
            self.begin_inline_edit_with(column, row, &text);
            return;
        }
        if clear {
            self.clear_selected_cell();
            return;
        }
        if let Some(text) = paste {
            self.paste_tsv(&text);
            return;
        }
        next.1 = next.1.min(rows.saturating_sub(1));
        if next != (column, row) {
            if extend {
                self.extend_selection(next.0, next.1);
            } else {
                self.select_cell(next.0, next.1);
            }
        }
    }
}

/// Background of a conditional style (light and dark theme variants).
fn conditional_fill(style: ConditionalStyle, dark: bool) -> Option<Color32> {
    match style {
        ConditionalStyle::RedFill => Some(if dark {
            Color32::from_rgb(92, 40, 46)
        } else {
            Color32::from_rgb(248, 215, 218)
        }),
        ConditionalStyle::YellowFill => Some(if dark {
            Color32::from_rgb(88, 75, 30)
        } else {
            Color32::from_rgb(255, 243, 205)
        }),
        ConditionalStyle::GreenFill => Some(if dark {
            Color32::from_rgb(38, 74, 45)
        } else {
            Color32::from_rgb(212, 237, 218)
        }),
        ConditionalStyle::RedText | ConditionalStyle::Bold => None,
    }
}

/// Text colour of a conditional style, when it changes it.
fn conditional_text(style: ConditionalStyle, dark: bool) -> Option<Color32> {
    match style {
        ConditionalStyle::RedText => Some(if dark {
            Color32::from_rgb(255, 150, 150)
        } else {
            Color32::from_rgb(176, 42, 55)
        }),
        _ => None,
    }
}
