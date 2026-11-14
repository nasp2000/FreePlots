// SPDX-License-Identifier: MIT
//! Tally: counts of the unique values of one or more columns.
//!
//! Each column is read in worksheet order; missing values are excluded and
//! counted. Numeric values are listed in ascending order, text and dates in
//! lexicographic order, with the count, percent, cumulative count and
//! cumulative percent.

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};

/// Counts the unique values of the selected columns.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for an empty or duplicated column list and
/// [`Error::EmptySample`] when no selected column has a value.
pub fn tally(df: &DataFrame, columns: &[String]) -> Result<AnalysisResult> {
    if columns.is_empty() {
        return Err(Error::Unsupported(
            "choose at least one column to tally".to_owned(),
        ));
    }
    let mut unique = columns.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != columns.len() {
        return Err(Error::Unsupported(
            "the column list contains duplicates".to_owned(),
        ));
    }
    let mut result = AnalysisResult::new(
        format!("Tally — {}", columns.join(", ")),
        "Counts of the unique values of every selected column".to_owned(),
    );
    let mut tables = 0usize;
    let mut largest = 0usize;
    let mut excluded = 0usize;
    for name in columns {
        let column = df.column(name)?;
        let (entries, missing) = counts(column);
        excluded += missing;
        let n: usize = entries.iter().map(|(_, count)| count).sum();
        if n == 0 {
            result.warnings.push(format!("column {name} has no values"));
            continue;
        }
        largest = largest.max(n);
        tables += 1;
        result.extra.push(Item::Message(name.clone()));
        result.extra.push(Item::Table {
            headers: vec![
                "Value".to_owned(),
                "Count".to_owned(),
                "Percent".to_owned(),
                "Cum. count".to_owned(),
                "Cum. percent".to_owned(),
            ],
            rows: entries
                .iter()
                .scan(0usize, |cumulative, (label, count)| {
                    *cumulative += count;
                    Some(vec![
                        label.clone(),
                        count.to_string(),
                        format!("{:.2}%", 100.0 * *count as f64 / n as f64),
                        cumulative.to_string(),
                        format!("{:.2}%", 100.0 * *cumulative as f64 / n as f64),
                    ])
                })
                .collect(),
        });
    }
    if tables == 0 {
        return Err(Error::EmptySample);
    }
    result.n = largest;
    result.excluded = excluded;
    result.statistics = vec![
        ("Variables", columns.len() as f64),
        ("Observations", largest as f64),
    ];
    result.notes = vec![
        (
            "Order".to_owned(),
            "numeric values in ascending order, text and dates lexicographically".to_owned(),
        ),
        (
            "Percent".to_owned(),
            "percent of the non-missing values of the column; the cumulative columns add the \
             counts and percents above"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "missing values are excluded and counted".to_owned(),
        ),
    ];
    result
        .warnings
        .extend(standard_warnings(largest, excluded, 0));
    Ok(result)
}

/// Distinct values of one column with their counts, and the missing count.
fn counts(column: &Column) -> (Vec<(String, usize)>, usize) {
    if matches!(
        column.dtype(),
        crate::data::DType::Float | crate::data::DType::Integer
    ) {
        let mut values = Vec::new();
        let mut missing = 0usize;
        for row in 0..column.len() {
            match column.value(row) {
                Some(value) => {
                    let label = column.label(row).unwrap_or_else(|| value.to_string());
                    values.push((value, label));
                }
                None => missing += 1,
            }
        }
        values.sort_by(|left, right| left.0.total_cmp(&right.0));
        let mut entries: Vec<(String, usize)> = Vec::new();
        let mut previous: Option<f64> = None;
        for (value, label) in values {
            if previous == Some(value) {
                if let Some((_, count)) = entries.last_mut() {
                    *count += 1;
                }
            } else {
                entries.push((label, 1));
                previous = Some(value);
            }
        }
        (entries, missing)
    } else {
        let mut map = std::collections::BTreeMap::<String, usize>::new();
        let mut missing = 0usize;
        for row in 0..column.len() {
            match column.label(row) {
                Some(label) => *map.entry(label).or_insert(0) += 1,
                None => missing += 1,
            }
        }
        (map.into_iter().collect(), missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    #[test]
    fn a_text_tally_counts_and_sorts_the_values() {
        let df = DataFrame::new(vec![Column::new_text(
            "grade",
            vec!["B", "A", "A", "B", "A", ""]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )])
        .expect("frame");
        let result = tally(&df, &["grade".to_owned()]).expect("tally");
        assert_eq!(result.n, 5);
        assert_eq!(result.excluded, 1);
        let (_, rows) = result
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { headers, rows } => Some((headers, rows)),
                _ => None,
            })
            .expect("table");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["A", "3", "60.00%", "3", "60.00%"]);
        assert_eq!(rows[1], vec!["B", "2", "40.00%", "5", "100.00%"]);
    }

    #[test]
    fn a_numeric_tally_sorts_by_value_not_by_text() {
        let df = DataFrame::new(vec![Column::new_f64(
            "count",
            vec![10.0, 2.0, 2.0, 10.0, 1.0],
        )])
        .expect("frame");
        let result = tally(&df, &["count".to_owned()]).expect("tally");
        let (_, rows) = result
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { headers, rows } => Some((headers, rows)),
                _ => None,
            })
            .expect("table");
        let values: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
        assert_eq!(values, vec!["1", "2", "10"]);
    }

    #[test]
    fn several_columns_produce_one_table_each() {
        let df = DataFrame::new(vec![
            Column::new_text(
                "first",
                vec!["A", "B", "A"].into_iter().map(str::to_owned).collect(),
            ),
            Column::new_f64("second", vec![1.0, 1.0, 2.0]),
        ])
        .expect("frame");
        let result = tally(&df, &["first".to_owned(), "second".to_owned()]).expect("tally");
        let tables = result
            .extra
            .iter()
            .filter(|item| matches!(item, Item::Table { .. }))
            .count();
        assert_eq!(tables, 2);
        let messages = result
            .extra
            .iter()
            .filter(|item| matches!(item, Item::Message(_)))
            .count();
        assert_eq!(messages, 2);
    }

    #[test]
    fn invalid_lists_and_empty_columns_are_handled() {
        let df = DataFrame::new(vec![Column::new_text(
            "g",
            vec!["".to_owned(), "".to_owned()],
        )])
        .expect("frame");
        assert!(tally(&df, &[]).is_err());
        assert!(tally(&df, &["g".to_owned(), "g".to_owned()]).is_err());
        assert!(matches!(
            tally(&df, &["g".to_owned()]),
            Err(Error::EmptySample)
        ));
    }
}
