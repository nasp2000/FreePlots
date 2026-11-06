//! Worksheet transformations: sort, transpose, stack, unstack and subset.
//!
//! These are pure operations on a [`DataFrame`]: they never read the disk and
//! never depend on the platform, so the same functions serve the command line
//! and the desktop interface.
//!
//! Missing values sort **last** whatever the direction, because they carry no
//! order. Numeric columns compare numerically, dates by their day number, text
//! and booleans lexicographically (`false` before `true`).

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};

/// Direction of a sort key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Smallest first.
    Ascending,
    /// Largest first.
    Descending,
}

impl SortDirection {
    /// Parses `asc`/`ascending` and `desc`/`descending`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "asc" | "ascending" | "up" => Some(Self::Ascending),
            "desc" | "descending" | "down" => Some(Self::Descending),
            _ => None,
        }
    }
}

/// Comparison key of one cell, used by the sort.
#[derive(Debug, Clone, PartialEq)]
enum Key {
    Number(f64),
    Text(String),
}

fn key_of(column: &Column, row: usize) -> Option<Key> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => Some(Key::Number(values[row])),
        ColumnData::I64(values) => Some(Key::Number(values[row] as f64)),
        ColumnData::Date(values) => Some(Key::Number(values[row] as f64)),
        ColumnData::Bool(values) => Some(Key::Text(
            if values[row] { "true" } else { "false" }.to_owned(),
        )),
        ColumnData::Text(values) => Some(Key::Text(values[row].clone())),
    }
}

fn compare_keys(left: Option<&Key>, right: Option<&Key>) -> std::cmp::Ordering {
    match (left, right) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(Key::Number(a)), Some(Key::Number(b))) => a.total_cmp(b),
        (Some(Key::Text(a)), Some(Key::Text(b))) => a.cmp(b),
        (Some(Key::Number(a)), Some(Key::Text(b))) => a.to_string().cmp(b),
        (Some(Key::Text(a)), Some(Key::Number(b))) => a.cmp(&b.to_string()),
    }
}

/// Sorts the rows by one or more columns.
///
/// The keys are applied in order: the second key breaks the ties of the first.
pub fn sort(df: &DataFrame, keys: &[(String, SortDirection)]) -> Result<DataFrame> {
    if keys.is_empty() {
        return Err(Error::Unsupported(
            "the sort needs at least one column".to_owned(),
        ));
    }
    let mut sources = Vec::with_capacity(keys.len());
    for (name, _) in keys {
        sources.push(df.column_index(name)?);
    }
    let mut order: Vec<usize> = (0..df.n_rows()).collect();
    let columns = df.columns();
    order.sort_by(|left, right| {
        for ((_, direction), index) in keys.iter().zip(&sources) {
            let column = &columns[*index];
            let a = key_of(column, *left);
            let b = key_of(column, *right);
            let mut ordering = compare_keys(a.as_ref(), b.as_ref());
            if *direction == SortDirection::Descending {
                // Missing values stay last: they are not reversed.
                ordering = match (a.is_none(), b.is_none()) {
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    _ => ordering.reverse(),
                };
            }
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.cmp(right)
    });
    let mut out = Vec::with_capacity(columns.len());
    for column in columns {
        out.push(reorder(column, &order)?);
    }
    DataFrame::new(out)
}

/// Builds a column with the rows taken from `column` in the given order,
/// keeping its type and its missing mask exactly.
fn reorder(column: &Column, order: &[usize]) -> Result<Column> {
    let missing: Vec<bool> = order.iter().map(|row| column.is_missing(*row)).collect();
    let data = match column.data() {
        ColumnData::F64(values) => ColumnData::F64(order.iter().map(|row| values[*row]).collect()),
        ColumnData::I64(values) => ColumnData::I64(order.iter().map(|row| values[*row]).collect()),
        ColumnData::Text(values) => {
            ColumnData::Text(order.iter().map(|row| values[*row].clone()).collect())
        }
        ColumnData::Bool(values) => {
            ColumnData::Bool(order.iter().map(|row| values[*row]).collect())
        }
        ColumnData::Date(values) => {
            ColumnData::Date(order.iter().map(|row| values[*row]).collect())
        }
    };
    Column::from_parts(column.name(), data, missing)
}

/// Transposes the worksheet: the columns become rows and the rows become
/// columns, named `R1`, `R2`, …; the first column of the result holds the
/// original column names.
pub fn transpose(df: &DataFrame) -> Result<DataFrame> {
    if df.n_rows() == 0 {
        return Err(Error::EmptySample);
    }
    let mut columns = Vec::with_capacity(df.n_rows() + 1);
    columns.push(Column::new_text(
        "Column",
        df.columns()
            .iter()
            .map(|column| column.name().to_owned())
            .collect(),
    ));
    for row in 0..df.n_rows() {
        let values: Vec<String> = df
            .columns()
            .iter()
            .map(|column| {
                if column.is_missing(row) {
                    String::new()
                } else {
                    column.cell_text(row)
                }
            })
            .collect();
        columns.push(Column::new_text(format!("R{}", row + 1), values));
    }
    DataFrame::new(columns)
}

/// Stacks the given columns into two: `Value` and `Variable`.
pub fn stack(df: &DataFrame, columns: &[String]) -> Result<DataFrame> {
    if columns.is_empty() {
        return Err(Error::Unsupported(
            "the stack needs at least one column".to_owned(),
        ));
    }
    let mut texts = Vec::new();
    let mut labels = Vec::new();
    for name in columns {
        let column = df.column(name)?;
        for row in 0..df.n_rows() {
            if column.is_missing(row) {
                continue;
            }
            texts.push(column.cell_text(row));
            labels.push(name.clone());
        }
    }
    if texts.is_empty() {
        return Err(Error::EmptySample);
    }
    let numeric = texts.iter().all(|text| text.parse::<f64>().is_ok());
    let value_column = if numeric {
        Column::new_f64(
            "Value",
            texts
                .iter()
                .map(|text| text.parse::<f64>().unwrap_or(f64::NAN))
                .collect(),
        )
    } else {
        Column::new_text("Value", texts)
    };
    DataFrame::new(vec![value_column, Column::new_text("Variable", labels)])
}

/// Unstacks a value column into one column per level of a categorical column.
///
/// Rows are matched by position: the `n`-th occurrence of a level receives the
/// `n`-th value of that level.
pub fn unstack(df: &DataFrame, value: &str, group: &str) -> Result<DataFrame> {
    if value == group {
        return Err(Error::Unsupported(
            "the value and the group must be different columns".to_owned(),
        ));
    }
    let value_column = df.column(value)?;
    let group_column = df.column(group)?;
    let mut levels: Vec<String> = Vec::new();
    let mut rows: Vec<(String, usize)> = Vec::new();
    for row in 0..df.n_rows() {
        if value_column.is_missing(row) || group_column.is_missing(row) {
            continue;
        }
        let label = group_column.cell_text(row);
        if !levels.contains(&label) {
            levels.push(label.clone());
        }
        rows.push((label, row));
    }
    if levels.is_empty() {
        return Err(Error::EmptySample);
    }
    levels.sort();
    let mut columns = Vec::with_capacity(levels.len());
    for level in &levels {
        let selected: Vec<usize> = rows
            .iter()
            .filter(|(label, _)| label == level)
            .map(|(_, row)| *row)
            .collect();
        let mut column = reorder(value_column, &selected)?;
        column.rename(level.clone());
        columns.push(column);
    }
    DataFrame::new(columns)
}

/// Operator of a subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsetOp {
    /// Equal (text comparison, or numeric when both are numbers).
    Equal,
    /// Different.
    NotEqual,
    /// Less than.
    Less,
    /// Less than or equal.
    LessOrEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterOrEqual,
    /// Text containment.
    Contains,
}

impl SubsetOp {
    /// Parses the operator from its symbol or name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "=" | "==" | "eq" => Some(Self::Equal),
            "!=" | "<>" | "ne" => Some(Self::NotEqual),
            "<" | "lt" => Some(Self::Less),
            "<=" | "le" => Some(Self::LessOrEqual),
            ">" | "gt" => Some(Self::Greater),
            ">=" | "ge" => Some(Self::GreaterOrEqual),
            "contains" | "~" => Some(Self::Contains),
            _ => None,
        }
    }

    /// Symbol used in the report and in the command line.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Contains => "contains",
        }
    }
}

fn matches(column: &Column, row: usize, op: SubsetOp, value: &str) -> bool {
    if column.is_missing(row) {
        return false;
    }
    let text = column.cell_text(row);
    let numeric = || text.parse::<f64>().ok().zip(value.parse::<f64>().ok());
    match op {
        SubsetOp::Equal => numeric().map_or_else(|| text == value, |(a, b)| (a - b).abs() < 1e-12),
        SubsetOp::NotEqual => match numeric() {
            Some((a, b)) => (a - b).abs() >= 1e-12,
            None => text != value,
        },
        SubsetOp::Less => numeric().is_some_and(|(a, b)| a < b),
        SubsetOp::LessOrEqual => numeric().is_some_and(|(a, b)| a <= b),
        SubsetOp::Greater => numeric().is_some_and(|(a, b)| a > b),
        SubsetOp::GreaterOrEqual => numeric().is_some_and(|(a, b)| a >= b),
        SubsetOp::Contains => text.contains(value),
    }
}

/// Keeps only the rows where `column <op> value`.
pub fn subset(df: &DataFrame, column: &str, op: SubsetOp, value: &str) -> Result<DataFrame> {
    let source = df.column(column)?;
    let keep: Vec<usize> = (0..df.n_rows())
        .filter(|row| matches(source, *row, op, value))
        .collect();
    if keep.is_empty() {
        return Err(Error::EmptySample);
    }
    let mut columns = Vec::with_capacity(df.n_columns());
    for column in df.columns() {
        columns.push(reorder(column, &keep)?);
    }
    DataFrame::new(columns)
}
