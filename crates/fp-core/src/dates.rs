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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::parse_date;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_date(
                "start",
                vec![
                    parse_date("2024-01-01").expect("date"),
                    parse_date("2024-02-29").expect("date"),
                    parse_date("2025-01-01").expect("date"),
                ],
            ),
            Column::new_date(
                "end",
                vec![
                    parse_date("2024-03-01").expect("date"),
                    parse_date("2025-01-01").expect("date"),
                    parse_date("2024-01-01").expect("date"),
                ],
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn the_difference_counts_calendar_days() {
        let values = difference(&frame(), "start", "end").expect("difference");
        assert_eq!(values[0], 60.0); // 2024-01-01 to 2024-03-01 (leap year)
        assert_eq!(values[1], 307.0);
        assert_eq!(values[2], -366.0);
    }

    #[test]
    fn the_shift_crosses_leap_years_and_goes_backwards() {
        let shifted = shift(&frame(), "start", 59).expect("shift");
        assert_eq!(shifted.dtype(), crate::data::DType::Date);
        assert_eq!(shifted.date(0), parse_date("2024-02-29"));
        assert_eq!(shifted.date(1), parse_date("2024-04-28"));
        let back = shift(&frame(), "start", -1).expect("shift");
        assert_eq!(back.date(0), parse_date("2023-12-31"));
    }

    #[test]
    fn missing_dates_stay_missing() {
        let df = DataFrame::new(vec![Column::new_date(
            "date",
            vec![parse_date("2024-01-01").expect("date"), i64::MIN],
        )])
        .expect("frame");
        let shifted = shift(&df, "date", 1).expect("shift");
        assert!(shifted.is_missing(1));
    }

    #[test]
    fn the_columns_are_added_and_invalid_inputs_rejected() {
        let mut df = frame();
        add_difference_column(&mut df, "start", "end", "days").expect("difference column");
        assert!(df.column("days").is_ok());
        add_shift_column(&mut df, "start", 30, "later").expect("shift column");
        assert_eq!(
            df.column("later").expect("later").dtype(),
            crate::data::DType::Date
        );
        assert!(add_difference_column(&mut df, "start", "end", "days").is_err());
        assert!(add_difference_column(&mut df, "start", "end", "").is_err());
        assert!(shift(&df, "days", 1).is_err());
    }
}
