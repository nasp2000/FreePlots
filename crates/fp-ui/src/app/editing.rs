//! Cell editing, the inline editor, selection and find/replace.

use super::*;

impl FreePlotsApp {
    pub fn paste_tsv(&mut self, text: &str) {
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return;
        }
        let width = lines
            .iter()
            .map(|line| line.split('\t').count())
            .max()
            .unwrap_or(1);
        let Some(df) = self.data.as_mut() else {
            return;
        };
        let before_rows = df.n_rows();
        // Grow the rows first: if that fails nothing has changed yet.
        if let Err(e) = df.ensure_rows(row + lines.len()) {
            self.set_error(e.to_string());
            return;
        }
        let first_new_column = df.n_columns();
        while df.n_columns() < column + width {
            df.add_column(fp_core::data::DType::Float);
        }
        let mut actions: Vec<EditAction> = (first_new_column..df.n_columns())
            .map(|index| add_column_action(df, index))
            .collect();
        let mut edits = Vec::new();
        for (r, line) in lines.iter().enumerate() {
            for (c, value) in line.split('\t').enumerate() {
                let target_column = column + c;
                let target_row = row + r;
                let before = cell_state(df, target_column, target_row);
                if let Err(e) = df.set_cell(target_column, target_row, value) {
                    // A refused cell makes the whole paste void: restore the
                    // cells already written, drop the rows and columns, and
                    // keep the older history intact.
                    rollback_cells(df, &edits, before_rows);
                    for action in actions.iter().rev() {
                        let _ = undo_action(df, action);
                    }
                    self.set_error(e.to_string());
                    return;
                }
                let after = cell_state(df, target_column, target_row);
                edits.push(CellEdit {
                    column: target_column,
                    row: target_row,
                    before,
                    after,
                    before_rows,
                });
            }
        }
        actions.push(EditAction::Cells(edits));
        self.push_undo(EditGroup { actions });
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Pasted {} × {} cells", lines.len(), width);
        self.select_cell(column, row);
    }
    pub fn commit_cell(&mut self) -> bool {
        let Some((column, row)) = self.edit_target else {
            self.set_error("Select a cell first");
            return false;
        };
        // A leading "=" evaluates a spreadsheet formula and stores its result.
        if self.edit_buffer.trim_start().starts_with('=') {
            let Some(df) = self.data.as_ref() else {
                self.set_error("No worksheet open");
                return false;
            };
            match fp_core::formula::evaluate(df, &self.edit_buffer) {
                Ok(value) => self.edit_buffer = value.text(),
                Err(error) => {
                    self.set_error(error.to_string());
                    return false;
                }
            }
        }
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return false;
        };
        let before_rows = df.n_rows();
        let before = cell_state(df, column, row);
        if row >= df.n_rows()
            && let Err(e) = df.ensure_rows(row + 1)
        {
            self.set_error(e.to_string());
            return false;
        }
        if let Err(e) = df.set_cell(column, row, &self.edit_buffer) {
            self.set_error(e.to_string());
            return false;
        }
        let after = cell_state(df, column, row);
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Edited C{} row {}", column + 1, row + 1);
        self.edit_buffer = df.cell_text(column, row).unwrap_or_default();
        if before != after || before_rows != df.n_rows() {
            self.push_undo(EditGroup {
                actions: vec![EditAction::Cells(vec![CellEdit {
                    column,
                    row,
                    before,
                    after,
                    before_rows,
                }])],
            });
        }
        self.refresh_column_summary();
        true
    }
    pub(super) fn set_cells(&mut self, cells: &[(usize, usize, String)]) -> bool {
        if cells.is_empty() {
            return false;
        }
        let Some(df) = self.data.as_mut() else {
            return false;
        };
        let before_rows = df.n_rows();
        let needed = cells.iter().map(|(_, row, _)| row + 1).max().unwrap_or(0);
        if needed > before_rows
            && let Err(e) = df.ensure_rows(needed)
        {
            self.set_error(e.to_string());
            return false;
        }
        let mut edits = Vec::with_capacity(cells.len());
        for (column, row, text) in cells {
            let before = cell_state(df, *column, *row);
            if let Err(e) = df.set_cell(*column, *row, text) {
                // The block is atomic: nothing of it survives the refusal.
                rollback_cells(df, &edits, before_rows);
                self.set_error(e.to_string());
                return false;
            }
            let after = cell_state(df, *column, *row);
            edits.push(CellEdit {
                column: *column,
                row: *row,
                before,
                after,
                before_rows,
            });
        }
        self.push_undo(EditGroup {
            actions: vec![EditAction::Cells(edits)],
        });
        self.edit_dirty = false;
        self.error = None;
        true
    }
    pub fn clear_selection(&mut self) {
        let Some((c0, r0, c1, r1)) = self.selection_bounds() else {
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        let before_rows = df.n_rows();
        let last_row = r1.min(df.n_rows().saturating_sub(1));
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        if last_row < r0 || last_column < c0 {
            return;
        }
        let mut edits = Vec::new();
        for row in r0..=last_row {
            for column in c0..=last_column {
                let before = cell_state(df, column, row);
                if before.is_none() {
                    continue;
                }
                if let Err(e) = df.set_missing(column, row) {
                    self.set_error(e.to_string());
                    return;
                }
                edits.push(CellEdit {
                    column,
                    row,
                    before,
                    after: None,
                    before_rows,
                });
            }
        }
        if edits.is_empty() {
            return;
        }
        let count = edits.len();
        self.push_undo(EditGroup {
            actions: vec![EditAction::Cells(edits)],
        });
        self.edit_buffer.clear();
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Cleared {count} cell(s)");
        self.refresh_column_summary();
    }
    pub fn find_next(&mut self) {
        let found = {
            let Some(df) = self.data.as_ref() else {
                return;
            };
            let needle = self.find_text.clone();
            if needle.is_empty() {
                None
            } else {
                let columns = df.n_columns();
                let rows = df.n_rows();
                if columns == 0 || rows == 0 {
                    None
                } else {
                    let start = self.selected_cell.unwrap_or((0, 0));
                    let total = columns * rows;
                    let mut found = None;
                    let mut index = start.1.min(rows - 1) * columns + start.0.min(columns - 1);
                    for _ in 0..total {
                        index = (index + 1) % total;
                        let column = index % columns;
                        let row = index / columns;
                        let text = df.cell_text(column, row).unwrap_or_default();
                        if text_matches(&text, &needle, self.find_match_case, self.find_whole_cell)
                        {
                            found = Some((column, row));
                            break;
                        }
                    }
                    found
                }
            }
        };
        match found {
            Some((column, row)) => {
                self.find_message = format!("Found at C{} R{}", column + 1, row + 1);
                self.select_cell(column, row);
            }
            None => {
                self.find_message = if self.find_text.is_empty() {
                    "Type the text to find".to_owned()
                } else {
                    "No matches".to_owned()
                };
            }
        }
    }
    pub fn replace_current(&mut self) {
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let text = df.cell_text(column, row).unwrap_or_default();
        let replaced = replace_in(
            &text,
            &self.find_text,
            &self.replace_text,
            self.find_match_case,
            self.find_whole_cell,
        );
        match replaced {
            Some(value) => {
                if self.set_cells(&[(column, row, value)]) {
                    self.status = "Replaced".to_owned();
                    self.find_next();
                }
            }
            None => self.find_next(),
        }
    }
    pub(super) fn refresh_column_summary(&mut self) {
        let Some((column, _)) = self.selected_cell else {
            self.column_summary.clear();
            return;
        };
        let Some(df) = self.data.as_ref() else {
            self.column_summary.clear();
            return;
        };
        let Some(column_ref) = df.columns().get(column) else {
            self.column_summary.clear();
            return;
        };
        let name = column_ref.name().to_owned();
        match column_ref.dtype() {
            fp_core::data::DType::Float | fp_core::data::DType::Integer => {
                match column_ref
                    .sample(fp_core::data::MissingPolicy::Drop)
                    .and_then(|s| fp_core::data::Descriptives::from_sample(&s))
                {
                    Ok(d) => {
                        self.column_summary = format!(
                            "{name}: n={}  mean={}  stdev={}  min={}  max={}",
                            d.n,
                            fmt_f64(d.mean),
                            fmt_f64(d.sd),
                            fmt_f64(d.min),
                            fmt_f64(d.max)
                        );
                    }
                    Err(_) => {
                        self.column_summary = format!("{name}: no numeric observations");
                    }
                }
            }
            dtype => {
                self.column_summary =
                    format!("{name}: {} column, {} rows", dtype.as_str(), df.n_rows());
            }
        }
    }
    pub(super) fn begin_inline_edit(&mut self, column: usize, row: usize) {
        self.select_cell(column, row);
        let missing = self
            .data
            .as_ref()
            .is_none_or(|df| df.cell_is_missing(column, row));
        self.inline_buffer = if missing {
            String::new()
        } else {
            self.edit_buffer.clone()
        };
        self.inline_edit = Some((column, row));
        self.inline_request_focus = true;
        self.edit_dirty = false;
    }
    pub(super) fn begin_inline_edit_with(&mut self, column: usize, row: usize, text: &str) {
        self.select_cell(column, row);
        self.inline_buffer = text.to_owned();
        self.inline_edit = Some((column, row));
        self.inline_request_focus = true;
        self.edit_dirty = false;
    }
    pub(super) fn commit_inline_edit(&mut self) -> bool {
        let Some((column, row)) = self.inline_edit else {
            return false;
        };
        self.edit_target = Some((column, row));
        self.edit_buffer = self.inline_buffer.clone();
        if self.commit_cell() {
            self.inline_edit = None;
            self.inline_request_focus = false;
            true
        } else {
            self.inline_request_focus = true;
            false
        }
    }
    pub fn select_cell(&mut self, column: usize, row: usize) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        let Some(df) = self.data.as_ref() else {
            self.selected_cell = None;
            self.selection_anchor = None;
            self.edit_target = None;
            self.edit_buffer.clear();
            self.column_summary.clear();
            return;
        };
        if df.n_columns() == 0 {
            self.selected_cell = None;
            self.selection_anchor = None;
            self.edit_target = None;
            self.edit_buffer.clear();
            self.column_summary.clear();
            return;
        }
        let column = column.min(df.n_columns() - 1);
        self.selected_cell = Some((column, row));
        self.selection_anchor = Some((column, row));
        self.edit_target = Some((column, row));
        self.selected = Some(df.columns()[column].name().to_owned());
        self.edit_buffer = if row < df.n_rows() {
            df.cell_text(column, row).unwrap_or_default()
        } else {
            String::new()
        };
        self.edit_dirty = false;
        self.refresh_column_summary();
    }
    pub fn select_row(&mut self, row: usize) {
        let Some(columns) = self.data.as_ref().map(DataFrame::n_columns) else {
            return;
        };
        if columns == 0 {
            return;
        }
        self.select_cell(0, row);
        self.extend_selection(columns - 1, row);
    }
    pub fn select_column(&mut self, column: usize) {
        let Some((columns, rows)) = self.data.as_ref().map(|df| (df.n_columns(), df.n_rows()))
        else {
            return;
        };
        if columns == 0 {
            return;
        }
        let column = column.min(columns - 1);
        self.select_cell(column, 0);
        self.extend_selection(column, rows.saturating_sub(1));
    }
    pub fn select_all_cells(&mut self) {
        let Some((columns, rows)) = self.data.as_ref().map(|df| (df.n_columns(), df.n_rows()))
        else {
            return;
        };
        if columns == 0 {
            return;
        }
        self.select_cell(0, 0);
        self.extend_selection(columns - 1, rows.saturating_sub(1));
    }
    pub fn extend_selection(&mut self, column: usize, row: usize) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        let Some(df) = self.data.as_ref() else {
            return;
        };
        if df.n_columns() == 0 {
            return;
        }
        let column = column.min(df.n_columns() - 1);
        if self.selection_anchor.is_none() {
            self.selection_anchor = self.selected_cell;
        }
        self.selected_cell = Some((column, row));
        self.edit_target = Some((column, row));
        self.selected = Some(df.columns()[column].name().to_owned());
        self.edit_buffer = if row < df.n_rows() {
            df.cell_text(column, row).unwrap_or_default()
        } else {
            String::new()
        };
        self.edit_dirty = false;
        self.refresh_column_summary();
    }
    pub fn selection_size(&self) -> (usize, usize) {
        self.selection_bounds()
            .map_or((0, 0), |(c0, r0, c1, r1)| (c1 - c0 + 1, r1 - r0 + 1))
    }
    pub fn selected_cell(&self) -> Option<(usize, usize)> {
        self.selected_cell
    }
    pub fn selection_tsv(&self) -> Option<String> {
        let (c0, r0, c1, r1) = self.selection_bounds()?;
        let df = self.data.as_ref()?;
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        let last_row = r1.min(df.n_rows().saturating_sub(1));
        if last_column < c0 || last_row < r0 {
            return None;
        }
        let mut lines = Vec::with_capacity(last_row - r0 + 1);
        for row in r0..=last_row {
            let mut cells = Vec::with_capacity(last_column - c0 + 1);
            for column in c0..=last_column {
                cells.push(cell_state(df, column, row).unwrap_or_default());
            }
            lines.push(cells.join("\t"));
        }
        Some(lines.join("\n"))
    }
    pub fn selected_column(&self) -> Option<&str> {
        self.selected.as_deref()
    }
    pub fn clear_selected_cell(&mut self) {
        self.clear_selection();
    }
}
