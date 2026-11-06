//! Date arithmetic on the calendar-date columns.
//!
//! Dates are stored as days since 1970-01-01 (see [`crate::data::parse_date`]),
//! so a difference is an exact number of days and a shift is an addition.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};

/// Difference `second − first` in days, one value per row.
///
/// A missing date on either side produces a missing (`NaN`) result.
///
/// # Errors
///
/// Returns [`Error::Csv`] when either column is not a date column.
pub fn difference(df: &DataFrame, first: &str, second: &str) -> Result<Vec<f64>> {
    let first_column = df.column(first)?;
    let second_column = df.column(second)?;
    if first_column.dtype() != crate::data::DType::Date
        || second_column.dtype() != crate::data::DType::Date
    {
        return Err(Error::Csv(
            "the date difference needs two date columns".to_owned(),
        ));
    }
    Ok((0..df.n_rows())
        .map(
            |row| match (first_column.date(row), second_column.date(row)) {
                (Some(first), Some(second)) => (second - first) as f64,
                _ => f64::NAN,
            },
        )
        .collect())
}

/// Shifts every date in `column` by `days` (negative shifts go backwards).
///
/// # Errors
///
/// Returns [`Error::Csv`] when the column is not a date column.
pub fn shift(df: &DataFrame, column: &str, days: i64) -> Result<Column> {
    let source = df.column(column)?;
    if source.dtype() != crate::data::DType::Date {
        return Err(Error::Csv(format!("column {column} is not a date column")));
    }
    let values: Vec<i64> = (0..df.n_rows())
        .map(|row| source.date(row).map_or(i64::MIN, |date| date + days))
        .collect();
    Ok(Column::new_date(column, values))
}

/// Adds the `second − first` difference as a numeric column.
///
/// # Errors
///
/// As [`difference`], plus [`Error::Unsupported`] when the name is empty or
/// already exists.
pub fn add_difference_column(
    df: &mut DataFrame,
    first: &str,
    second: &str,
    name: &str,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Unsupported("name the new column".to_owned()));
    }
    if df.column(name).is_ok() {
        return Err(Error::Unsupported(format!("column {name} already exists")));
    }
    let values = difference(df, first, second)?;
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))?;
    Ok(())
}

/// Adds the shifted dates as a date column.
///
/// # Errors
///
/// As [`shift`], plus [`Error::Unsupported`] when the name is empty or
/// already exists.
pub fn add_shift_column(df: &mut DataFrame, column: &str, days: i64, name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Unsupported("name the new column".to_owned()));
    }
    if df.column(name).is_ok() {
        return Err(Error::Unsupported(format!("column {name} already exists")));
    }
    let shifted = shift(df, column, days)?;
    let mut shifted = shifted;
    shifted.rename(name);
    let index = df.n_columns();
    df.insert_column(index, shifted)?;
    Ok(())
}
