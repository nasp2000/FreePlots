//! Batch descriptive statistics over a folder of CSV files.
//!
//! The file system belongs to the application layer; this module receives the
//! already-loaded frames (or the read error per file) and builds one Session
//! report with a row per file, so a folder of daily or per-line sheets can be
//! checked at a glance.

use crate::data::{DataFrame, MissingPolicy};
use crate::report::{AnalysisResult, Item, standard_warnings};

/// One file of the batch: its display name and the loaded frame, or the error
/// that prevented loading it.
pub type BatchInput = (String, Result<DataFrame, String>);

/// Builds the batch report for one numeric column.
#[must_use]
pub fn describe(files: &[BatchInput], column: &str) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Batch descriptive statistics — column {column}"),
        "One row per CSV file in the folder; n−1 standard deviation",
    );
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(files.len());
    let mut loaded = 0usize;
    let mut failed = 0usize;
    let mut observations = 0usize;
    let mut excluded = 0usize;
    for (name, frame) in files {
        match frame {
            Ok(frame) => match frame.column(column) {
                Ok(values) => match values.sample(MissingPolicy::Drop) {
                    Ok(sample) if sample.n() > 0 => {
                        loaded += 1;
                        observations += sample.n();
                        excluded += sample.excluded();
                        let mean = sample
                            .mean()
                            .map_or_else(|_| "—".to_owned(), |v| format!("{v:.6}"));
                        let sd = sample
                            .sd()
                            .map_or_else(|_| "—".to_owned(), |v| format!("{v:.6}"));
                        let min = sample
                            .min()
                            .map_or_else(|_| "—".to_owned(), |v| format!("{v:.6}"));
                        let max = sample
                            .max()
                            .map_or_else(|_| "—".to_owned(), |v| format!("{v:.6}"));
                        rows.push(vec![
                            name.clone(),
                            sample.n().to_string(),
                            mean,
                            sd,
                            min,
                            max,
                            String::new(),
                        ]);
                    }
                    Ok(_) => {
                        failed += 1;
                        rows.push(row(name, "no observations"));
                    }
                    Err(error) => {
                        failed += 1;
                        rows.push(row(name, &error.to_string()));
                    }
                },
                Err(_) => {
                    failed += 1;
                    rows.push(row(name, "column missing"));
                }
            },
            Err(error) => {
                failed += 1;
                rows.push(row(name, error));
            }
        }
    }
    result.n = observations;
    result.excluded = excluded;
    result.statistics = vec![
        ("Files", files.len() as f64),
        ("Loaded", loaded as f64),
        ("Failed", failed as f64),
        ("Observations", observations as f64),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "File".to_owned(),
            "N".to_owned(),
            "Mean".to_owned(),
            "StDev".to_owned(),
            "Min".to_owned(),
            "Max".to_owned(),
            "Note".to_owned(),
        ],
        rows,
    });
    result.notes = vec![(
        "Ordering".to_owned(),
        "files in folder order, one row per file".to_owned(),
    )];
    result.warnings = standard_warnings(observations, excluded, 0);
    if failed > 0 {
        result.warnings.push(format!(
            "{failed} file(s) could not be summarised; the Note column says why"
        ));
    }
    result
}

fn row(name: &str, note: &str) -> Vec<String> {
    vec![
        name.to_owned(),
        "—".to_owned(),
        "—".to_owned(),
        "—".to_owned(),
        "—".to_owned(),
        "—".to_owned(),
        note.to_owned(),
    ]
}
