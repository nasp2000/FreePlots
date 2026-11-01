//! Data: columns, table and clean numeric sample.
//!
//! Explicit semantics (PLAN.md, section 6.1):
//! **empty ≠ 0 ≠ `NaN`**. A missing value is recorded as such and the policy
//! applied is reported in the result.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::special::{chi_square_quantile, student_t_quantile};

/// What to do with non-finite observations (`NaN`, `±inf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingPolicy {
    /// Remove and count the non-finite observations.
    Drop,
    /// Any non-finite value is an error.
    Error,
}

/// Formats days since 1970-01-01 as an ISO 8601 date.
#[must_use]
pub fn format_date(days: i64) -> String {
    format_date_with(days, DateFormat::Iso)
}

/// Display format of a calendar-date column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DateFormat {
    /// `YYYY-MM-DD` (the stored form).
    #[default]
    Iso,
    /// `DD/MM/YYYY`.
    DayMonthYear,
    /// `MM/DD/YYYY`.
    MonthDayYear,
    /// `DD Mon YYYY` (for example `29 Feb 2024`).
    DayMonthNameYear,
}

impl DateFormat {
    /// Name used in the menu and the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Iso => "ISO (YYYY-MM-DD)",
            Self::DayMonthYear => "DD/MM/YYYY",
            Self::MonthDayYear => "MM/DD/YYYY",
            Self::DayMonthNameYear => "DD Mon YYYY",
        }
    }

    /// Stable identifier used by the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Iso => "iso",
            Self::DayMonthYear => "dmy",
            Self::MonthDayYear => "mdy",
            Self::DayMonthNameYear => "d-mon-y",
        }
    }
}

const MONTH_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats a calendar date (days since 1970-01-01) in the chosen style.
#[must_use]
pub fn format_date_with(days: i64, format: DateFormat) -> String {
    if days == i64::MIN {
        return "*".to_owned();
    }
    // Days-from-civil (Howard Hinnant's algorithm), inverted.
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    match format {
        DateFormat::Iso => format!("{y:04}-{m:02}-{d:02}"),
        DateFormat::DayMonthYear => format!("{d:02}/{m:02}/{y:04}"),
        DateFormat::MonthDayYear => format!("{m:02}/{d:02}/{y:04}"),
        DateFormat::DayMonthNameYear => {
            format!("{d:02} {} {y:04}", MONTH_NAMES[(m - 1) as usize])
        }
    }
}

/// Parses an ISO 8601 date (`YYYY-MM-DD`) into days since 1970-01-01.
#[must_use]
pub fn parse_date(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.len() != 10 {
        return None;
    }
    let bytes = text.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = text[0..4].parse().ok()?;
    let month: i64 = text[5..7].parse().ok()?;
    let day: i64 = text[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days-from-civil (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe as i64 - 719_468)
}

/// Logical type of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DType {
    /// Floating point.
    Float,
    /// Integer.
    Integer,
    /// Text.
    Text,
    /// Boolean.
    Bool,
    /// Calendar date (ISO 8601, no time).
    Date,
}

impl DType {
    /// Stable name, used in the report and in the tests.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Float => "float",
            Self::Integer => "integer",
            Self::Text => "text",
            Self::Bool => "bool",
            Self::Date => "date",
        }
    }
}

/// Content of a column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "values", rename_all = "snake_case")]
pub enum ColumnData {
    /// Real values.
    F64(Vec<f64>),
    /// Integer values.
    I64(Vec<i64>),
    /// Text.
    Text(Vec<String>),
    /// Booleans.
    Bool(Vec<bool>),
    /// Calendar dates, as days since 1970-01-01.
    Date(Vec<i64>),
}

impl ColumnData {
    fn len(&self) -> usize {
        match self {
            Self::F64(v) => v.len(),
            Self::I64(v) => v.len(),
            Self::Text(v) => v.len(),
            Self::Bool(v) => v.len(),
            Self::Date(v) => v.len(),
        }
    }

    fn dtype(&self) -> DType {
        match self {
            Self::F64(_) => DType::Float,
            Self::I64(_) => DType::Integer,
            Self::Text(_) => DType::Text,
            Self::Bool(_) => DType::Bool,
            Self::Date(_) => DType::Date,
        }
    }
}

/// A data column, with a missing-value mask.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Column {
    name: String,
    data: ColumnData,
    missing: Vec<bool>,
}

impl Column {
    /// Creates an `f64` column, marking everything that is not finite as missing.
    #[must_use]
    pub fn new_f64(name: impl Into<String>, values: Vec<f64>) -> Self {
        let missing = values.iter().map(|v| !v.is_finite()).collect();
        Self {
            name: name.into(),
            data: ColumnData::F64(values),
            missing,
        }
    }

    /// Creates a text column.
    #[must_use]
    pub fn new_text(name: impl Into<String>, values: Vec<String>) -> Self {
        let missing = values.iter().map(String::is_empty).collect();
        Self {
            name: name.into(),
            data: ColumnData::Text(values),
            missing,
        }
    }

    /// Creates a date column from days since 1970-01-01 (`i64::MIN` is missing).
    #[must_use]
    pub fn new_date(name: impl Into<String>, values: Vec<i64>) -> Self {
        let missing = values.iter().map(|value| *value == i64::MIN).collect();
        Self {
            name: name.into(),
            data: ColumnData::Date(values),
            missing,
        }
    }

    /// Creates a column from existing data and mask.
    ///
    /// The mask must have the same length as the data.
    pub fn from_parts(
        name: impl Into<String>,
        data: ColumnData,
        missing: Vec<bool>,
    ) -> Result<Self> {
        if missing.len() != data.len() {
            return Err(Error::Project(format!(
                "column {}: invalid mask with {} entries for {} values",
                name.into(),
                missing.len(),
                data.len()
            )));
        }
        Ok(Self {
            name: name.into(),
            data,
            missing,
        })
    }

    /// Column name (`C1`, `C2`, …).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Logical type.
    #[must_use]
    pub fn dtype(&self) -> DType {
        self.data.dtype()
    }

    /// Number of rows (including missing).
    #[must_use]
    pub fn len(&self) -> usize {
        self.missing.len()
    }

    /// `true` if the column has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `true` if the row is missing.
    #[must_use]
    pub fn is_missing(&self, row: usize) -> bool {
        self.missing.get(row).copied().unwrap_or(true)
    }

    /// Raw content of the column.
    #[must_use]
    pub fn data(&self) -> &ColumnData {
        &self.data
    }

    /// Renames the column.
    pub fn rename(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Text shown for a cell; missing cells are `*`.
    #[must_use]
    pub fn cell_text(&self, row: usize) -> String {
        if self.is_missing(row) {
            return "*".to_owned();
        }
        match &self.data {
            ColumnData::F64(values) => values[row].to_string(),
            ColumnData::I64(values) => values[row].to_string(),
            ColumnData::Text(values) => values[row].clone(),
            ColumnData::Bool(values) => {
                if values[row] {
                    "T".to_owned()
                } else {
                    "F".to_owned()
                }
            }
            ColumnData::Date(values) => format_date(values[row]),
        }
    }

    /// Replaces one cell from the text typed in the worksheet.
    ///
    /// Empty text and `*` mark a missing value. A numeric or boolean column
    /// rejects text it cannot parse instead of silently turning the whole
    /// column into text; the old value is kept on error.
    pub fn set_text(&mut self, row: usize, text: &str) -> Result<()> {
        if row >= self.len() {
            return Err(Error::Cell {
                column: self.name.clone(),
                row: row + 1,
                message: "row is outside the column".to_owned(),
            });
        }
        let trimmed = text.trim();
        let missing = trimmed.is_empty() || (self.dtype() != DType::Text && trimmed == "*");

        match &mut self.data {
            ColumnData::F64(values) => {
                if missing {
                    self.missing[row] = true;
                } else {
                    let value = parse_f64(trimmed).ok_or_else(|| Error::Cell {
                        column: self.name.clone(),
                        row: row + 1,
                        message: "expected a number, `*` for missing or empty".to_owned(),
                    })?;
                    if !value.is_finite() {
                        return Err(Error::Cell {
                            column: self.name.clone(),
                            row: row + 1,
                            message: "the value is not finite".to_owned(),
                        });
                    }
                    values[row] = value;
                    self.missing[row] = false;
                }
            }
            ColumnData::I64(values) => {
                if missing {
                    self.missing[row] = true;
                } else {
                    let value = parse_i64(trimmed).ok_or_else(|| Error::Cell {
                        column: self.name.clone(),
                        row: row + 1,
                        message: "expected an integer, `*` for missing or empty".to_owned(),
                    })?;
                    values[row] = value;
                    self.missing[row] = false;
                }
            }
            ColumnData::Date(values) => {
                if missing {
                    self.missing[row] = true;
                } else {
                    let value = parse_date(trimmed).ok_or_else(|| Error::Cell {
                        column: self.name.clone(),
                        row: row + 1,
                        message: "expected a date as YYYY-MM-DD, `*` for missing or empty"
                            .to_owned(),
                    })?;
                    values[row] = value;
                    self.missing[row] = false;
                }
            }
            ColumnData::Text(values) => {
                if text.trim().is_empty() {
                    self.missing[row] = true;
                } else {
                    values[row] = text.to_owned();
                    self.missing[row] = false;
                }
            }
            ColumnData::Bool(values) => {
                if missing {
                    self.missing[row] = true;
                } else {
                    let value = parse_bool(trimmed).ok_or_else(|| Error::Cell {
                        column: self.name.clone(),
                        row: row + 1,
                        message: "expected T/F, true/false, yes/no, 1/0 or `*`".to_owned(),
                    })?;
                    values[row] = value;
                    self.missing[row] = false;
                }
            }
        }
        Ok(())
    }

    /// Appends one missing cell.
    pub fn push_missing(&mut self) {
        match &mut self.data {
            ColumnData::F64(values) => values.push(0.0),
            ColumnData::I64(values) => values.push(0),
            ColumnData::Text(values) => values.push(String::new()),
            ColumnData::Bool(values) => values.push(false),
            ColumnData::Date(values) => values.push(i64::MIN),
        }
        self.missing.push(true);
    }

    /// Inserts one missing cell at `row`.
    pub fn insert_missing(&mut self, row: usize) -> Result<()> {
        if row > self.len() {
            return Err(Error::Cell {
                column: self.name.clone(),
                row: row + 1,
                message: "insert position is outside the column".to_owned(),
            });
        }
        match &mut self.data {
            ColumnData::F64(values) => values.insert(row, 0.0),
            ColumnData::I64(values) => values.insert(row, 0),
            ColumnData::Text(values) => values.insert(row, String::new()),
            ColumnData::Bool(values) => values.insert(row, false),
            ColumnData::Date(values) => values.insert(row, i64::MIN),
        }
        self.missing.insert(row, true);
        Ok(())
    }

    /// Marks one cell as missing without changing its stored value.
    pub fn set_missing(&mut self, row: usize) -> Result<()> {
        if row >= self.len() {
            return Err(Error::Cell {
                column: self.name.clone(),
                row: row + 1,
                message: "row is outside the column".to_owned(),
            });
        }
        self.missing[row] = true;
        Ok(())
    }

    /// Truncates the column to at most `rows` rows.
    pub fn truncate(&mut self, rows: usize) {
        if rows >= self.len() {
            return;
        }
        match &mut self.data {
            ColumnData::F64(values) => values.truncate(rows),
            ColumnData::I64(values) => values.truncate(rows),
            ColumnData::Text(values) => values.truncate(rows),
            ColumnData::Bool(values) => values.truncate(rows),
            ColumnData::Date(values) => values.truncate(rows),
        }
        self.missing.truncate(rows);
    }

    /// Removes one cell.
    pub fn remove_row(&mut self, row: usize) -> Result<()> {
        if row >= self.len() {
            return Err(Error::Cell {
                column: self.name.clone(),
                row: row + 1,
                message: "row is outside the column".to_owned(),
            });
        }
        match &mut self.data {
            ColumnData::F64(values) => {
                values.remove(row);
            }
            ColumnData::I64(values) => {
                values.remove(row);
            }
            ColumnData::Text(values) => {
                values.remove(row);
            }
            ColumnData::Bool(values) => {
                values.remove(row);
            }
            ColumnData::Date(values) => {
                values.remove(row);
            }
        }
        self.missing.remove(row);
        Ok(())
    }

    /// Row-aligned numeric values: the value of every row, with `NaN` where
    /// the cell is missing or not numeric.
    ///
    /// Use this, not [`Column::numeric`], when the values are indexed by the
    /// original row number: `numeric` compacts the column, so a missing value
    /// would shift every later row.
    #[must_use]
    pub fn numeric_by_row(&self) -> Vec<f64> {
        (0..self.len())
            .map(|row| self.value(row).unwrap_or(f64::NAN))
            .collect()
    }

    /// Numeric value of one row, `None` when the cell is missing or the column
    /// is not numeric.
    #[must_use]
    pub fn value(&self, row: usize) -> Option<f64> {
        if self.is_missing(row) {
            return None;
        }
        match &self.data {
            ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
            ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
        }
    }

    /// Calendar date of one row as days since 1970-01-01.
    ///
    /// `None` when the cell is missing or the column is not a date column.
    #[must_use]
    pub fn date(&self, row: usize) -> Option<i64> {
        if self.is_missing(row) {
            return None;
        }
        match &self.data {
            ColumnData::Date(values) => values.get(row).copied().filter(|value| *value != i64::MIN),
            _ => None,
        }
    }

    /// Text label of one row: the text itself, the ISO date or the number as
    /// text. `None` when the cell is missing.
    #[must_use]
    pub fn label(&self, row: usize) -> Option<String> {
        if self.is_missing(row) {
            return None;
        }
        match &self.data {
            ColumnData::Text(values) => values.get(row).cloned(),
            ColumnData::Date(values) => values
                .get(row)
                .map(|value| crate::data::format_date(*value)),
            ColumnData::I64(values) => values.get(row).map(ToString::to_string),
            ColumnData::F64(values) => values.get(row).map(ToString::to_string),
            ColumnData::Bool(values) => values
                .get(row)
                .map(|value| if *value { "T" } else { "F" }.to_owned()),
        }
    }

    /// Extracts the valid numeric values, applying the given policy.
    pub fn numeric(&self, policy: MissingPolicy) -> Result<Vec<f64>> {
        match &self.data {
            ColumnData::F64(values) => {
                let mut out = Vec::with_capacity(values.len());
                for (i, &v) in values.iter().enumerate() {
                    if self.is_missing(i) || !v.is_finite() {
                        match policy {
                            MissingPolicy::Error => {
                                return Err(Error::NonFinite { index: i, value: v });
                            }
                            MissingPolicy::Drop => {}
                        }
                    } else {
                        out.push(v);
                    }
                }
                Ok(out)
            }
            ColumnData::I64(values) => {
                let mut out = Vec::with_capacity(values.len());
                for (i, &v) in values.iter().enumerate() {
                    if !self.is_missing(i) {
                        out.push(v as f64);
                    } else if policy == MissingPolicy::Error {
                        return Err(Error::NonFinite {
                            index: i,
                            value: v as f64,
                        });
                    }
                }
                Ok(out)
            }
            ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => {
                Err(Error::Csv(format!(
                    "column {} is of type {} and cannot be used as a numeric sample",
                    self.name,
                    self.data.dtype().as_str()
                )))
            }
        }
    }

    /// Builds a [`Sample`] from this column.
    ///
    /// The excluded count includes both masked cells and non-finite values
    /// that [`Column::numeric`] dropped.
    pub fn sample(&self, policy: MissingPolicy) -> Result<Sample> {
        let values = self.numeric(policy)?;
        let excluded = self.len().saturating_sub(values.len());
        Sample::from_valid(values, excluded)
    }
}

fn parse_f64(text: &str) -> Option<f64> {
    let normalized = if text.contains(',') && !text.contains('.') {
        text.replace(',', ".")
    } else {
        text.to_owned()
    };
    normalized.parse::<f64>().ok()
}

fn parse_i64(text: &str) -> Option<i64> {
    if let Ok(value) = text.parse::<i64>() {
        return Some(value);
    }
    let value = parse_f64(text)?;
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        Some(value as i64)
    } else {
        None
    }
}

fn parse_bool(text: &str) -> Option<bool> {
    match text.to_ascii_lowercase().as_str() {
        "t" | "true" | "yes" | "y" | "1" => Some(true),
        "f" | "false" | "no" | "n" | "0" => Some(false),
        _ => None,
    }
}

/// A table: ordered set of columns of the same length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataFrame {
    columns: Vec<Column>,
}

impl DataFrame {
    /// Creates a table, checking that all columns have the same length.
    pub fn new(columns: Vec<Column>) -> Result<Self> {
        let mut n: Option<usize> = None;
        for c in &columns {
            match n {
                None => n = Some(c.len()),
                Some(expected) if expected != c.len() => {
                    return Err(Error::Project(format!(
                        "column {} has {} rows, expected {}",
                        c.name(),
                        c.len(),
                        expected
                    )));
                }
                Some(_) => {}
            }
        }
        Ok(Self { columns })
    }

    /// Creates a blank worksheet with `C1..Cn` numeric columns and `rows` empty
    /// rows.
    pub fn blank(columns: usize, rows: usize) -> Result<Self> {
        let columns = (1..=columns)
            .map(|index| Column::new_f64(format!("C{index}"), vec![f64::NAN; rows]))
            .collect();
        Self::new(columns)
    }

    /// Grows the table to at least `rows` rows, appending missing cells.
    ///
    /// A worksheet with no columns gains `C1`; this is what lets the UI type
    /// into the first spare cell of a new worksheet.
    pub fn ensure_rows(&mut self, rows: usize) -> Result<()> {
        if self.columns.is_empty() && rows > 0 {
            self.columns.push(Column::new_f64("C1", Vec::new()));
        }
        while self.n_rows() < rows {
            for column in &mut self.columns {
                column.push_missing();
            }
        }
        Ok(())
    }

    /// Appends one empty row.
    pub fn add_row(&mut self) -> Result<usize> {
        if self.columns.is_empty() {
            self.columns.push(Column::new_f64("C1", Vec::new()));
        }
        self.ensure_rows(self.n_rows() + 1)?;
        Ok(self.n_rows() - 1)
    }

    /// Inserts one empty row at `row`.
    pub fn insert_row(&mut self, row: usize) -> Result<()> {
        if row > self.n_rows() {
            return Err(Error::Project(format!(
                "row {} is outside a worksheet with {} rows",
                row + 1,
                self.n_rows()
            )));
        }
        if self.columns.is_empty() {
            self.columns.push(Column::new_f64("C1", Vec::new()));
        }
        for column in &mut self.columns {
            column.insert_missing(row)?;
        }
        Ok(())
    }

    /// Removes one row from every column.
    pub fn remove_row(&mut self, row: usize) -> Result<()> {
        if row >= self.n_rows() {
            return Err(Error::Project(format!(
                "row {} is outside a worksheet with {} rows",
                row + 1,
                self.n_rows()
            )));
        }
        for column in &mut self.columns {
            column.remove_row(row)?;
        }
        Ok(())
    }

    /// Adds a column with the next free `Cn` name.
    pub fn add_column(&mut self, dtype: DType) -> usize {
        let mut index = self.n_columns() + 1;
        while self
            .columns
            .iter()
            .any(|c| c.name().eq_ignore_ascii_case(&format!("C{index}")))
        {
            index += 1;
        }
        let name = format!("C{index}");
        let rows = self.n_rows();
        let data = match dtype {
            DType::Float => ColumnData::F64(vec![f64::NAN; rows]),
            DType::Integer => ColumnData::I64(vec![0; rows]),
            DType::Text => ColumnData::Text(vec![String::new(); rows]),
            DType::Bool => ColumnData::Bool(vec![false; rows]),
            DType::Date => ColumnData::Date(vec![i64::MIN; rows]),
        };
        let missing = vec![true; rows];
        self.columns
            .push(Column::from_parts(name, data, missing).expect("matching column lengths"));
        self.n_columns() - 1
    }

    /// Removes a column.
    pub fn remove_column(&mut self, index: usize) -> Result<()> {
        if index >= self.n_columns() {
            return Err(Error::Project(format!(
                "column index {index} is outside a worksheet with {} columns",
                self.n_columns()
            )));
        }
        self.columns.remove(index);
        Ok(())
    }

    /// Inserts a column at `index`, preserving a worksheet invariant.
    pub fn insert_column(&mut self, index: usize, column: Column) -> Result<()> {
        if index > self.n_columns() {
            return Err(Error::Project(format!(
                "column index {index} is outside a worksheet with {} columns",
                self.n_columns()
            )));
        }
        if !self.columns.is_empty() && column.len() != self.n_rows() {
            return Err(Error::Project(format!(
                "column {} has {} rows, expected {}",
                column.name(),
                column.len(),
                self.n_rows()
            )));
        }
        if self
            .columns
            .iter()
            .any(|existing| existing.name().eq_ignore_ascii_case(column.name()))
        {
            return Err(Error::Project(format!(
                "a column named '{}' already exists",
                column.name()
            )));
        }
        self.columns.insert(index, column);
        Ok(())
    }

    /// Inserts an empty column with the given name and type.
    pub fn insert_empty_column(
        &mut self,
        index: usize,
        name: impl Into<String>,
        dtype: DType,
    ) -> Result<()> {
        let name = name.into();
        let rows = self.n_rows();
        let data = match dtype {
            DType::Float => ColumnData::F64(vec![f64::NAN; rows]),
            DType::Integer => ColumnData::I64(vec![0; rows]),
            DType::Text => ColumnData::Text(vec![String::new(); rows]),
            DType::Bool => ColumnData::Bool(vec![false; rows]),
            DType::Date => ColumnData::Date(vec![i64::MIN; rows]),
        };
        let missing = vec![true; rows];
        let column = Column::from_parts(name, data, missing)?;
        self.insert_column(index, column)
    }

    /// Renames a column, rejecting empty or duplicate names.
    pub fn rename_column(&mut self, index: usize, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Project("a column name cannot be empty".to_owned()));
        }
        let n_columns = self.n_columns();
        if index >= n_columns {
            return Err(Error::Project(format!(
                "column index {index} is outside a worksheet with {n_columns} columns"
            )));
        }
        if self
            .columns
            .iter()
            .enumerate()
            .any(|(i, c)| i != index && c.name().eq_ignore_ascii_case(name))
        {
            return Err(Error::Project(format!(
                "a column named '{name}' already exists"
            )));
        }
        self.columns[index].rename(name);
        Ok(())
    }

    /// Replaces one cell from worksheet text.
    pub fn set_cell(&mut self, column: usize, row: usize, text: &str) -> Result<()> {
        let n_columns = self.n_columns();
        if column >= n_columns {
            return Err(Error::Project(format!(
                "column index {column} is outside a worksheet with {n_columns} columns"
            )));
        }
        self.columns[column].set_text(row, text)
    }

    /// Marks one cell as missing.
    pub fn set_missing(&mut self, column: usize, row: usize) -> Result<()> {
        let n_columns = self.n_columns();
        if column >= n_columns {
            return Err(Error::Project(format!(
                "column index {column} is outside a worksheet with {n_columns} columns"
            )));
        }
        self.columns[column].set_missing(row)
    }

    /// `true` if the cell is missing or outside the table.
    #[must_use]
    pub fn cell_is_missing(&self, column: usize, row: usize) -> bool {
        self.columns.get(column).is_none_or(|c| c.is_missing(row))
    }

    /// Truncates every column to at most `rows` rows.
    pub fn truncate_rows(&mut self, rows: usize) {
        for column in &mut self.columns {
            column.truncate(rows);
        }
    }

    /// Text shown for one cell; missing cells are `*`.
    #[must_use]
    pub fn cell_text(&self, column: usize, row: usize) -> Option<String> {
        self.columns.get(column).map(|c| c.cell_text(row))
    }

    /// Columns in order.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Number of columns.
    #[must_use]
    pub fn n_columns(&self) -> usize {
        self.columns.len()
    }

    /// Number of rows.
    #[must_use]
    pub fn n_rows(&self) -> usize {
        self.columns.first().map_or(0, Column::len)
    }

    /// Looks up a column by name.
    pub fn column(&self, name: &str) -> Result<&Column> {
        self.columns
            .iter()
            .find(|c| c.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::ColumnNotFound(name.to_owned()))
    }

    /// Index of a column by name.
    pub fn column_index(&self, name: &str) -> Result<usize> {
        self.columns
            .iter()
            .position(|c| c.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::ColumnNotFound(name.to_owned()))
    }
}

/// Clean numeric sample: only finite values, with a count of those excluded.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    values: Vec<f64>,
    excluded: usize,
}

impl Sample {
    /// Creates a sample applying the missing-value policy.
    pub fn new(values: Vec<f64>, policy: MissingPolicy) -> Result<Self> {
        let mut out = Vec::with_capacity(values.len());
        let mut excluded = 0usize;
        for (index, v) in values.into_iter().enumerate() {
            if v.is_finite() {
                out.push(v);
            } else {
                match policy {
                    MissingPolicy::Error => return Err(Error::NonFinite { index, value: v }),
                    MissingPolicy::Drop => excluded += 1,
                }
            }
        }
        if out.is_empty() {
            return Err(Error::EmptySample);
        }
        Ok(Self {
            values: out,
            excluded,
        })
    }

    /// Creates a sample from values that are already clean, with an explicit
    /// count of observations that were excluded before this call.
    pub fn from_valid(values: Vec<f64>, excluded: usize) -> Result<Self> {
        if values.is_empty() {
            return Err(Error::EmptySample);
        }
        if let Some((index, value)) = values
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| !value.is_finite())
        {
            return Err(Error::NonFinite { index, value });
        }
        Ok(Self { values, excluded })
    }

    /// Valid observations, in the original order.
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Number of valid observations.
    #[must_use]
    pub fn n(&self) -> usize {
        self.values.len()
    }

    /// Observations excluded for not being finite.
    #[must_use]
    pub fn excluded(&self) -> usize {
        self.excluded
    }

    /// Sum.
    #[must_use]
    pub fn sum(&self) -> f64 {
        self.values.iter().sum()
    }

    /// Largest magnitude in the sample, used to rescale before summing.
    fn scale(&self) -> f64 {
        self.values
            .iter()
            .fold(0.0_f64, |largest, value| largest.max(value.abs()))
    }

    /// Arithmetic mean.
    pub fn mean(&self) -> Result<f64> {
        if self.values.is_empty() {
            return Err(Error::EmptySample);
        }
        let mean = self.sum() / self.n() as f64;
        if mean.is_finite() {
            return Ok(mean);
        }
        // The plain sum overflowed: divide by the largest magnitude first and
        // multiply the average back.
        let scale = self.scale();
        if scale == 0.0 {
            return Ok(0.0);
        }
        let scaled = self.values.iter().map(|value| value / scale).sum::<f64>();
        Ok(scale * scaled / self.n() as f64)
    }

    /// Sample variance (divisor `n − 1`).
    pub fn variance(&self) -> Result<f64> {
        if self.n() < 2 {
            return Err(Error::SampleTooSmall {
                n: self.n(),
                min: 2,
            });
        }
        let mean = self.mean()?;
        let ss: f64 = self.values.iter().map(|v| (v - mean) * (v - mean)).sum();
        Ok(ss / (self.n() - 1) as f64)
    }

    /// Sample standard deviation (divisor `n − 1`).
    ///
    /// Values around 1e300 overflow the sum of squares even though their
    /// standard deviation is representable, so a non-finite variance is
    /// recomputed from values divided by the largest magnitude.
    pub fn sd(&self) -> Result<f64> {
        let variance = self.variance()?;
        if variance.is_finite() {
            return Ok(variance.sqrt());
        }
        let scale = self.scale();
        if scale == 0.0 {
            return Ok(0.0);
        }
        let scaled: Vec<f64> = self.values.iter().map(|value| value / scale).collect();
        let mean = scaled.iter().sum::<f64>() / self.n() as f64;
        let ss: f64 = scaled
            .iter()
            .map(|value| (value - mean) * (value - mean))
            .sum();
        Ok(scale * (ss / (self.n() - 1) as f64).sqrt())
    }

    /// Population variance (divisor `n`), used by the Normal MLE estimator.
    pub fn variance_population(&self) -> Result<f64> {
        if self.values.is_empty() {
            return Err(Error::EmptySample);
        }
        let mean = self.mean()?;
        let ss: f64 = self.values.iter().map(|v| (v - mean) * (v - mean)).sum();
        let variance = ss / self.n() as f64;
        if variance.is_finite() {
            return Ok(variance);
        }
        let scale = self.scale();
        if scale == 0.0 {
            return Ok(0.0);
        }
        let scaled: Vec<f64> = self.values.iter().map(|v| v / scale).collect();
        let scaled_mean = scaled.iter().sum::<f64>() / self.n() as f64;
        let ss: f64 = scaled
            .iter()
            .map(|v| (v - scaled_mean) * (v - scaled_mean))
            .sum();
        Ok(scale * scale * ss / self.n() as f64)
    }

    /// Minimum.
    pub fn min(&self) -> Result<f64> {
        self.values
            .iter()
            .copied()
            .reduce(f64::min)
            .ok_or(Error::EmptySample)
    }

    /// Maximum.
    pub fn max(&self) -> Result<f64> {
        self.values
            .iter()
            .copied()
            .reduce(f64::max)
            .ok_or(Error::EmptySample)
    }

    /// Median (mean of the two central values when `n` is even).
    pub fn median(&self) -> Result<f64> {
        self.quantile(0.5)
    }

    /// Quantile by linear interpolation between order statistics, with the
    /// plotting position `h = (n + 1)p`.
    ///
    /// `p = 0` is the minimum and `p = 1` the maximum. This is the
    /// interpolated convention of the captured run, confirmed by its
    /// descriptive statistics (n = 20: Q1 19.4198, Q3 21.7969).
    pub fn quantile(&self, p: f64) -> Result<f64> {
        quantile_sorted(&self.sorted(), p)
    }

    /// Sorted copy (ascending).
    #[must_use]
    pub fn sorted(&self) -> Vec<f64> {
        let mut v = self.values.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Number of ties (repeated values) in the sorted sample.
    #[must_use]
    pub fn ties(&self) -> usize {
        let sorted = self.sorted();
        let mut ties = 0usize;
        for w in sorted.windows(2) {
            if w[0] == w[1] {
                ties += 1;
            }
        }
        ties
    }
}

/// Descriptive statistics of a sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptives {
    /// Valid observations.
    pub n: usize,
    /// Excluded observations.
    pub excluded: usize,
    /// Mean.
    pub mean: f64,
    /// Sample standard deviation (`n − 1`).
    pub sd: f64,
    /// Minimum.
    pub min: f64,
    /// Median.
    pub median: f64,
    /// Maximum.
    pub max: f64,
}

impl Descriptives {
    /// Computes the descriptives of a sample.
    pub fn from_sample(sample: &Sample) -> Result<Self> {
        Ok(Self {
            n: sample.n(),
            excluded: sample.excluded(),
            mean: sample.mean()?,
            sd: sample.sd()?,
            min: sample.min()?,
            median: sample.median()?,
            max: sample.max()?,
        })
    }
}

fn quantile_sorted(sorted: &[f64], p: f64) -> Result<f64> {
    if !p.is_finite() || !(0.0..=1.0).contains(&p) {
        return Err(Error::InvalidParameter {
            name: "quantile probability",
            value: p,
        });
    }
    let n = sorted.len();
    if n == 0 {
        return Err(Error::EmptySample);
    }
    if n == 1 {
        return Ok(sorted[0]);
    }
    // One-based position h = (n + 1)p, clamped to the sample range.
    let h = (n as f64 + 1.0) * p;
    if h <= 1.0 {
        return Ok(sorted[0]);
    }
    if h >= n as f64 {
        return Ok(sorted[n - 1]);
    }
    let floor = h.floor();
    let fraction = h - floor;
    let lower = floor as usize - 1;
    let upper = (lower + 1).min(n - 1);
    Ok(sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction)
}

/// Descriptive summary with quartiles and a confidence interval for the mean.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSummary {
    /// Valid observations.
    pub n: usize,
    /// Excluded observations.
    pub excluded: usize,
    /// Mean.
    pub mean: f64,
    /// Sample standard deviation (`n − 1`), absent for `n = 1`.
    pub sd: Option<f64>,
    /// Standard error of the mean, absent for `n = 1`.
    pub se_mean: Option<f64>,
    /// Minimum.
    pub min: f64,
    /// First quartile (interpolated, `h = (n + 1)p`).
    pub q1: f64,
    /// Median.
    pub median: f64,
    /// Third quartile (interpolated, `h = (n + 1)p`).
    pub q3: f64,
    /// Maximum.
    pub max: f64,
    /// Interquartile range (`q3 − q1`).
    pub iqr: f64,
    /// Two-sided confidence interval for the mean, absent for `n = 1`.
    pub ci_mean: Option<(f64, f64)>,
    /// Distribution-free confidence interval for the median, absent when the
    /// sample is too small for the requested confidence.
    pub ci_median: Option<(f64, f64)>,
    /// Chi-square confidence interval for the standard deviation, absent for
    /// `n = 1` (and assuming a Normal sample).
    pub ci_sd: Option<(f64, f64)>,
    /// Sample skewness (the standardised third central moment), absent for
    /// fewer than three observations.
    pub skewness: Option<f64>,
    /// Standard error of the skewness, absent for fewer than three
    /// observations.
    pub skewness_standard_error: Option<f64>,
    /// Sample excess kurtosis (the fourth central moment over the squared
    /// variance, minus three), absent for fewer than four observations.
    pub excess_kurtosis: Option<f64>,
    /// Standard error of the excess kurtosis, absent for fewer than four
    /// observations.
    pub kurtosis_standard_error: Option<f64>,
}

impl ColumnSummary {
    /// Summarizes a clean sample.
    ///
    /// `confidence` is the two-sided confidence level for the mean (typically
    /// `0.95`). The interval uses Student t with `n − 1` degrees of freedom.
    pub fn from_sample(sample: &Sample, confidence: f64) -> Result<Self> {
        if !confidence.is_finite() || !(0.0..1.0).contains(&confidence) {
            return Err(Error::InvalidParameter {
                name: "confidence level",
                value: confidence,
            });
        }
        let n = sample.n();
        let mean = sample.mean()?;
        let sd = if n >= 2 { Some(sample.sd()?) } else { None };
        let se_mean = sd.map(|s| s / (n as f64).sqrt());
        let ci_mean = se_mean.map(|se| {
            let t = student_t_quantile(1.0 - (1.0 - confidence) / 2.0, (n - 1) as f64);
            (mean - t * se, mean + t * se)
        });
        let sorted = sample.sorted();
        let q1 = quantile_sorted(&sorted, 0.25)?;
        let median = quantile_sorted(&sorted, 0.5)?;
        let q3 = quantile_sorted(&sorted, 0.75)?;
        let ci_median = median_confidence_interval(&sorted, confidence);
        // Written as `sd · √(df/χ²)` so a very large sd does not overflow
        // when squared.
        let ci_sd = sd.map(|sd| {
            let df = (n - 1) as f64;
            let alpha = 1.0 - confidence;
            let lower = sd * (df / chi_square_quantile(1.0 - alpha / 2.0, df)).sqrt();
            let upper = sd * (df / chi_square_quantile(alpha / 2.0, df)).sqrt();
            (lower, upper)
        });
        let (skewness, skewness_standard_error) = if n >= 3 {
            let (m2, m3, _) = central_moments(&sorted, n);
            let skewness = if m2 > 0.0 && m2.is_finite() && m3.is_finite() {
                m3 / m2.powf(1.5)
            } else {
                f64::NAN
            };
            let nf = n as f64;
            let standard_error =
                (6.0 * nf * (nf - 1.0) / ((nf - 2.0) * (nf + 1.0) * (nf + 3.0))).sqrt();
            (Some(skewness), Some(standard_error))
        } else {
            (None, None)
        };
        let (excess_kurtosis, kurtosis_standard_error) = if n >= 4 {
            let (m2, _, m4) = central_moments(&sorted, n);
            let kurtosis = if m2 > 0.0 && m2.is_finite() && m4.is_finite() {
                m4 / (m2 * m2) - 3.0
            } else {
                f64::NAN
            };
            let nf = n as f64;
            let standard_error = (24.0 * nf * (nf - 1.0).powi(2)
                / ((nf - 3.0) * (nf - 2.0) * (nf - 1.0) * (nf + 3.0) * (nf + 5.0)))
                .sqrt();
            (Some(kurtosis), Some(standard_error))
        } else {
            (None, None)
        };
        Ok(Self {
            n,
            excluded: sample.excluded(),
            mean,
            sd,
            se_mean,
            min: sorted[0],
            q1,
            median,
            q3,
            max: sorted[n - 1],
            iqr: q3 - q1,
            ci_mean,
            ci_median,
            ci_sd,
            skewness,
            skewness_standard_error,
            excess_kurtosis,
            kurtosis_standard_error,
        })
    }
}

/// The second, third and fourth central moments of a sorted sample.
///
/// The moments use the divisor `n` (the "moment" convention).
fn central_moments(sorted: &[f64], n: usize) -> (f64, f64, f64) {
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let mut m2 = 0.0;
    let mut m3 = 0.0;
    let mut m4 = 0.0;
    for value in sorted {
        let deviation = value - mean;
        let squared = deviation * deviation;
        m2 += squared;
        m3 += squared * deviation;
        m4 += squared * squared;
    }
    let nf = n as f64;
    (m2 / nf, m3 / nf, m4 / nf)
}

/// Distribution-free confidence interval for the median.
///
/// The order-statistic interval `[x₍ₖ₎, x₍ₙ₊₁₋ₖ₎]` covers the population
/// median with probability `1 − 2·P(Binomial(n, ½) ≤ k − 1)`; the largest
/// `k` whose coverage reaches the requested level is used. The interval is
/// `None` when even the widest one (`k = 1`) does not reach it.
pub fn median_confidence_interval(sorted: &[f64], confidence: f64) -> Option<(f64, f64)> {
    let n = sorted.len();
    if n < 2 || !confidence.is_finite() || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    let mut best = None;
    for k in 1..=(n / 2) {
        // P(Binomial(n, ½) ≤ k − 1) = I_{1/2}(n − k + 1, k).
        let lower_tail = crate::special::regularized_beta(0.5, (n - k + 1) as f64, k as f64);
        let coverage = 1.0 - 2.0 * lower_tail;
        if coverage >= confidence {
            best = Some(k);
        } else {
            break;
        }
    }
    let k = best?;
    Some((sorted[k - 1], sorted[n - k]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(values: &[f64]) -> Sample {
        Sample::new(values.to_vec(), MissingPolicy::Drop).expect("valid sample")
    }

    #[test]
    fn numeric_by_row_keeps_the_rows_aligned() {
        // A missing value in the middle must leave a hole, not shift the rest.
        let column = Column::new_f64("x", vec![1.0, f64::NAN, 3.0, 4.0]);
        let values = column.numeric_by_row();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0], 1.0);
        assert!(values[1].is_nan());
        assert_eq!(values[2], 3.0);
        assert_eq!(values[3], 4.0);
        // The compacting extractor is still the right tool for statistics.
        assert_eq!(
            column.numeric(MissingPolicy::Drop).expect("values"),
            vec![1.0, 3.0, 4.0]
        );
        assert_eq!(column.value(2), Some(3.0));
        assert_eq!(column.value(1), None);
        assert_eq!(column.value(9), None);
    }

    #[test]
    fn value_and_label_refuse_the_wrong_column_types() {
        let text = Column::new_text("t", vec!["a".into(), "b".into()]);
        assert_eq!(text.value(0), None);
        assert_eq!(text.label(0).as_deref(), Some("a"));
        let number = Column::new_f64("x", vec![2.5, 3.5]);
        assert_eq!(number.value(1), Some(3.5));
        assert_eq!(number.label(1).as_deref(), Some("3.5"));
        let flag = Column::from_parts("b", ColumnData::Bool(vec![true, false]), vec![false, false])
            .expect("bool column");
        assert_eq!(flag.value(0), None);
        assert_eq!(flag.label(0).as_deref(), Some("T"));
        assert_eq!(flag.label(1).as_deref(), Some("F"));
        let date = Column::new_date("d", vec![0, 1]);
        assert_eq!(date.value(0), None);
        assert!(date.label(0).is_some());
    }

    #[test]
    fn the_skewness_and_kurtosis_follow_their_definitions() {
        // A symmetric series has no skewness and no excess kurtosis.
        let s = sample(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        let summary = ColumnSummary::from_sample(&s, 0.95).expect("summary");
        assert!(summary.skewness.expect("skewness").abs() < 1e-12);
        // The excess kurtosis of five equally spaced points is −1.3.
        assert!((summary.excess_kurtosis.expect("kurtosis") + 1.3).abs() < 1e-12);
        assert!(summary.skewness_standard_error.expect("se") > 0.0);

        // A right-skewed series has a positive skewness.
        let s = sample(&[1.0, 1.0, 1.0, 1.0, 10.0]);
        let summary = ColumnSummary::from_sample(&s, 0.95).expect("summary");
        assert!(summary.skewness.expect("skewness") > 1.0);
    }

    #[test]
    fn median_confidence_interval_uses_the_order_statistics() {
        let values: Vec<f64> = (1..=10).map(|i| i as f64).collect();
        let interval = median_confidence_interval(&values, 0.95).expect("interval");
        // With n = 10 the 95 % interval is [x₍₂₎, x₍₉₎] = [2, 9].
        assert_eq!(interval, (2.0, 9.0));
        let wider = median_confidence_interval(&values, 0.99).expect("interval");
        // At 99 % only k = 1 survives: [x₍₁₎, x₍₁₀₎].
        assert_eq!(wider, (1.0, 10.0));
        assert!(median_confidence_interval(&[1.0], 0.95).is_none());
    }

    #[test]
    fn median_confidence_interval_covers_the_exact_probability() {
        // The coverage of [x₍ₖ₎, x₍ₙ₊₁₋ₖ₎] is 1 − 2·P(Bin(n, ½) ≤ k−1);
        // for n = 20 and k = 3 that is 1 − 2·211/1048576 = 0.99960.
        let values: Vec<f64> = (1..=20).map(|i| i as f64).collect();
        let interval = median_confidence_interval(&values, 0.999).expect("interval");
        assert_eq!(interval, (3.0, 18.0));
    }

    #[test]
    fn known_mean_and_standard_deviation() {
        let s = sample(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert!((s.mean().unwrap() - 5.0).abs() < 1e-15);
        // sample standard deviation of this classic series = sqrt(32/7)
        assert!((s.sd().unwrap() - (32.0f64 / 7.0).sqrt()).abs() < 1e-15);
        assert!((s.variance_population().unwrap() - 4.0).abs() < 1e-15);
    }

    #[test]
    fn even_and_odd_median() {
        assert_eq!(sample(&[1.0, 2.0, 3.0]).median().unwrap(), 2.0);
        assert_eq!(sample(&[1.0, 2.0, 3.0, 4.0]).median().unwrap(), 2.5);
    }

    #[test]
    fn empty_is_an_error() {
        assert_eq!(
            Sample::new(vec![], MissingPolicy::Drop).unwrap_err(),
            Error::EmptySample
        );
    }

    #[test]
    fn non_finite_values_are_counted_or_rejected() {
        let drop = Sample::new(vec![1.0, f64::NAN, 3.0], MissingPolicy::Drop).unwrap();
        assert_eq!(drop.n(), 2);
        assert_eq!(drop.excluded(), 1);
        assert_eq!(
            Sample::new(vec![1.0, f64::INFINITY], MissingPolicy::Error).unwrap_err(),
            Error::NonFinite {
                index: 1,
                value: f64::INFINITY
            }
        );
    }

    #[test]
    fn n_one_has_no_standard_deviation() {
        let s = sample(&[42.0]);
        assert_eq!(s.sd().unwrap_err(), Error::SampleTooSmall { n: 1, min: 2 });
    }

    #[test]
    fn numerically_stable_variance() {
        // Naive variance (E[x²] − E[x]²) fails here; ours does not.
        let s = sample(&[1e8 + 1.0, 1e8 + 2.0, 1e8 + 3.0]);
        assert!((s.variance().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ties_are_detected() {
        assert_eq!(sample(&[1.0, 1.0, 2.0, 2.0, 2.0]).ties(), 3);
        assert_eq!(sample(&[1.0, 2.0]).ties(), 0);
    }

    #[test]
    fn column_sample_counts_missing_and_non_finite_values() {
        let column = Column::new_f64("C1", vec![1.0, f64::NAN, 3.0]);
        let sample = column.sample(MissingPolicy::Drop).unwrap();
        assert_eq!(sample.n(), 2);
        assert_eq!(sample.excluded(), 1);
        assert_eq!(sample.values(), &[1.0, 3.0]);
    }

    #[test]
    fn text_column_is_not_numeric() {
        let c = Column::new_text("C1", vec!["a".into(), "b".into()]);
        assert!(matches!(c.numeric(MissingPolicy::Drop), Err(Error::Csv(_))));
    }

    #[test]
    fn table_rejects_columns_of_different_lengths() {
        let df = DataFrame::new(vec![
            Column::new_f64("C1", vec![1.0, 2.0]),
            Column::new_f64("C2", vec![1.0]),
        ]);
        assert!(df.is_err());
    }

    #[test]
    fn descriptives_of_small_sample() {
        let d = Descriptives::from_sample(&sample(&[1.0, 2.0, 3.0])).unwrap();
        assert_eq!(d.n, 3);
        assert_eq!(d.min, 1.0);
        assert_eq!(d.max, 3.0);
        assert_eq!(d.median, 2.0);
    }

    #[test]
    fn quantiles_use_the_interpolated_convention() {
        let s = sample(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(s.quantile(0.0).unwrap(), 1.0);
        // h = (n + 1)p = 1.5 and 4.5 for the quartiles.
        assert_eq!(s.quantile(0.25).unwrap(), 1.5);
        assert_eq!(s.quantile(0.5).unwrap(), 3.0);
        assert_eq!(s.quantile(0.75).unwrap(), 4.5);
        assert_eq!(s.quantile(1.0).unwrap(), 5.0);
        assert!(s.quantile(1.1).is_err());
    }

    #[test]
    fn quartiles_match_the_captured_descriptive_statistics() {
        // n = 20 dataset of the parity kit: the captured run prints
        // Q1 = 19.4198 and Q3 = 21.7969.
        let values = [
            20.62715749,
            15.91402014,
            20.71665986,
            18.42692322,
            24.57605812,
            21.89088436,
            18.68555526,
            20.04259395,
            19.25743458,
            21.51474668,
            20.02615504,
            21.08730919,
            20.64233209,
            23.67040944,
            20.18208664,
            20.78376531,
            23.56650505,
            25.48269896,
            19.90698872,
            18.02129698,
        ];
        let s = sample(&values);
        let summary = ColumnSummary::from_sample(&s, 0.95).unwrap();
        assert!((summary.q1 - 19.419824).abs() < 1e-6, "{}", summary.q1);
        assert!((summary.q3 - 21.796850).abs() < 1e-6, "{}", summary.q3);
        assert!((summary.median - 20.634745).abs() < 1e-6);
    }

    #[test]
    fn summary_reports_sd_quartiles_and_mean_ci() {
        let s = sample(&[1.0, 2.0, 3.0, 4.0]);
        let summary = ColumnSummary::from_sample(&s, 0.95).unwrap();
        assert_eq!(summary.n, 4);
        assert_eq!(summary.mean, 2.5);
        assert_eq!(summary.q1, 1.25);
        assert_eq!(summary.median, 2.5);
        assert_eq!(summary.q3, 3.75);
        assert_eq!(summary.iqr, 2.5);
        let (low, high) = summary.ci_mean.expect("95% CI");
        assert!(low < summary.mean && summary.mean < high);
        assert!((high - low - 3.182446305284263 * summary.sd.unwrap()).abs() < 1e-9);
    }

    #[test]
    fn summary_of_one_observation_has_no_sd_or_ci() {
        let summary = ColumnSummary::from_sample(&sample(&[42.0]), 0.95).unwrap();
        assert_eq!(summary.mean, 42.0);
        assert!(summary.sd.is_none());
        assert!(summary.se_mean.is_none());
        assert!(summary.ci_mean.is_none());
        assert_eq!(summary.q1, 42.0);
        assert_eq!(summary.q3, 42.0);
    }

    #[test]
    fn worksheet_cells_can_be_edited() {
        let mut df = DataFrame::new(vec![
            Column::new_f64("C1", vec![f64::NAN; 2]),
            Column::new_text("C2", vec![String::new(); 2]),
        ])
        .unwrap();
        df.set_cell(0, 0, "12,5").unwrap();
        df.set_cell(0, 1, "*").unwrap();
        df.set_cell(1, 0, "batch A").unwrap();

        assert_eq!(df.cell_text(0, 0).as_deref(), Some("12.5"));
        assert_eq!(df.cell_text(0, 1).as_deref(), Some("*"));
        assert!(df.columns()[0].is_missing(1));
        assert_eq!(
            df.columns()[0].numeric(MissingPolicy::Drop).unwrap(),
            vec![12.5]
        );
        assert_eq!(df.cell_text(1, 0).as_deref(), Some("batch A"));
    }

    #[test]
    fn invalid_numeric_edit_keeps_the_old_value() {
        let mut df = DataFrame::new(vec![Column::new_f64("C1", vec![1.0])]).unwrap();
        let err = df.set_cell(0, 0, "not a number").unwrap_err();
        assert!(matches!(err, Error::Cell { .. }));
        assert_eq!(df.columns()[0].numeric(MissingPolicy::Drop).unwrap(), [1.0]);
    }

    #[test]
    fn rows_and_columns_can_be_added_and_removed() {
        let mut df = DataFrame::blank(2, 2).unwrap();
        let row = df.add_row().unwrap();
        assert_eq!(row, 2);
        assert_eq!(df.n_rows(), 3);

        df.insert_row(1).unwrap();
        assert_eq!(df.n_rows(), 4);
        df.remove_row(0).unwrap();
        assert_eq!(df.n_rows(), 3);

        let index = df.add_column(DType::Text);
        assert_eq!(index, 2);
        assert_eq!(df.n_columns(), 3);
        assert_eq!(df.n_rows(), 3);
        df.rename_column(2, "batch").unwrap();
        assert_eq!(df.columns()[2].name(), "batch");
        df.remove_column(1).unwrap();
        assert_eq!(df.n_columns(), 2);
    }

    #[test]
    fn blank_worksheet_gains_its_first_column_on_typing() {
        let mut df = DataFrame::new(vec![]).unwrap();
        df.ensure_rows(1).unwrap();
        assert_eq!(df.n_columns(), 1);
        assert_eq!(df.n_rows(), 1);
        df.set_cell(0, 0, "4.5").unwrap();
        assert_eq!(df.cell_text(0, 0).as_deref(), Some("4.5"));
    }

    #[test]
    fn columns_can_be_inserted_and_reinserted() {
        let mut df = DataFrame::blank(2, 2).unwrap();
        let column = df.columns()[1].clone();
        df.remove_column(1).unwrap();
        df.insert_column(1, column).unwrap();
        assert_eq!(df.n_columns(), 2);
        df.insert_empty_column(1, "text", DType::Text).unwrap();
        assert_eq!(df.n_columns(), 3);
        assert_eq!(df.columns()[1].dtype(), DType::Text);
        assert!(df.insert_empty_column(0, "C1", DType::Float).is_err());
    }

    #[test]
    fn cells_can_be_marked_missing_and_rows_truncated() {
        let mut df = DataFrame::blank(2, 3).unwrap();
        df.set_cell(0, 0, "1").unwrap();
        df.set_missing(0, 0).unwrap();
        assert!(df.cell_is_missing(0, 0));
        assert_eq!(df.cell_text(0, 0).as_deref(), Some("*"));

        df.truncate_rows(1);
        assert_eq!(df.n_rows(), 1);
        df.truncate_rows(5);
        assert_eq!(df.n_rows(), 1);
    }

    #[test]
    fn the_standard_deviation_interval_matches_scipy() {
        let sample =
            Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop).expect("sample");
        let summary = ColumnSummary::from_sample(&sample, 0.95).expect("summary");
        let (low, high) = summary.ci_sd.expect("interval");
        assert!((low - 0.947_312_670_675_223_6).abs() < 1e-6, "{low}");
        assert!((high - 4.543_490_393_448_267).abs() < 1e-6, "{high}");
        let two = Sample::new(vec![1.0, 2.0], MissingPolicy::Drop).expect("sample");
        let summary = ColumnSummary::from_sample(&two, 0.95).expect("summary");
        let (low, high) = summary.ci_sd.expect("interval");
        assert!((low - 0.315_475_114_078_284_2).abs() < 1e-6, "{low}");
        assert!((high - 22.563_890_064_876_563).abs() < 1e-6, "{high}");
        let one = Sample::new(vec![1.0], MissingPolicy::Drop).expect("sample");
        assert!(
            ColumnSummary::from_sample(&one, 0.95)
                .expect("summary")
                .ci_sd
                .is_none()
        );
    }

    #[test]
    fn the_date_formats_render_the_same_calendar_day() {
        let day = parse_date("2024-02-29").expect("date");
        assert_eq!(format_date_with(day, DateFormat::Iso), "2024-02-29");
        assert_eq!(
            format_date_with(day, DateFormat::DayMonthYear),
            "29/02/2024"
        );
        assert_eq!(
            format_date_with(day, DateFormat::MonthDayYear),
            "02/29/2024"
        );
        assert_eq!(
            format_date_with(day, DateFormat::DayMonthNameYear),
            "29 Feb 2024"
        );
        assert_eq!(format_date_with(i64::MIN, DateFormat::Iso), "*");
        assert_eq!(DateFormat::DayMonthYear.name(), "DD/MM/YYYY");
    }

    #[test]
    fn column_names_must_be_unique_and_non_empty() {
        let mut df = DataFrame::blank(2, 0).unwrap();
        assert!(df.rename_column(0, "  ").is_err());
        assert!(df.rename_column(1, "C1").is_err());
        df.rename_column(0, "weight").unwrap();
        assert_eq!(df.columns()[0].name(), "weight");
    }
}
