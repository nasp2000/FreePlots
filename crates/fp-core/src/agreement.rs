//! Attribute agreement analysis.
//!
//! Three columns describe the study: the **part**, the **appraiser** and the
//! **rating** (categorical, typically pass/fail). The report measures the
//! agreement in two ways, both with Cohen's kappa:
//!
//! - **Within each appraiser**: the ratings of the same part are split into
//!   the first and the second repetition, and kappa is computed over the parts
//!   (a part with fewer than two ratings by that appraiser is skipped).
//! - **Between appraisers**: every part gets the modal (most frequent) rating
//!   of each appraiser, and kappa is computed over the parts for every pair of
//!   appraisers.
//!
//! `kappa = (P₀ − Pₑ)/(1 − Pₑ)`, where `P₀` is the observed agreement and
//! `Pₑ` the agreement expected from the margins. A kappa of one means perfect
//! agreement, zero the agreement expected by chance.
//!
//! The comparison run has an attribute agreement analysis; its repetition
//! and between-appraiser rules were not captured, so the numbers are not
//! claimed as reference parity — see `docs/parity.md`.

use std::collections::BTreeMap;

use crate::data::{Column, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Cohen's kappa of two paired ratings.
#[derive(Debug, Clone, PartialEq)]
pub struct Kappa {
    /// Pairs used.
    pub n: usize,
    /// Observed agreement.
    pub observed: f64,
    /// Agreement expected from the margins.
    pub expected: f64,
    /// Cohen's kappa.
    pub kappa: f64,
}

/// Within-appraiser agreement.
#[derive(Debug, Clone, PartialEq)]
pub struct WithinAppraiser {
    /// Appraiser label.
    pub appraiser: String,
    /// Parts with two ratings.
    pub parts: usize,
    /// Cohen's kappa of the two repetitions.
    pub kappa: Kappa,
}

/// Between-appraiser agreement for one pair.
#[derive(Debug, Clone, PartialEq)]
pub struct BetweenAppraisers {
    /// First appraiser.
    pub first: String,
    /// Second appraiser.
    pub second: String,
    /// Cohen's kappa of the modal ratings.
    pub kappa: Kappa,
}

/// Result of the attribute agreement analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Agreement {
    /// Parts in the study.
    pub parts: Vec<String>,
    /// Appraisers in the study.
    pub appraisers: Vec<String>,
    /// Rows used.
    pub n: usize,
    /// Within-appraiser agreement.
    pub within: Vec<WithinAppraiser>,
    /// Between-appraiser agreement.
    pub between: Vec<BetweenAppraisers>,
}

/// Label of a row of a column (`None` when the cell is missing).
fn label(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    Some(column.cell_text(row))
}

/// Cohen's kappa of two rating vectors of the same length.
fn cohen_kappa(first: &[String], second: &[String]) -> Kappa {
    let n = first.len().min(second.len());
    if n == 0 {
        return Kappa {
            n: 0,
            observed: f64::NAN,
            expected: f64::NAN,
            kappa: f64::NAN,
        };
    }
    let mut observed = 0usize;
    let mut first_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut second_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for index in 0..n {
        if first[index] == second[index] {
            observed += 1;
        }
        *first_counts.entry(first[index].as_str()).or_insert(0) += 1;
        *second_counts.entry(second[index].as_str()).or_insert(0) += 1;
    }
    let nf = n as f64;
    let po = observed as f64 / nf;
    let mut pe = 0.0;
    for (category, count) in &first_counts {
        let other = second_counts.get(category).copied().unwrap_or(0);
        pe += (*count as f64 / nf) * (other as f64 / nf);
    }
    let kappa = if (1.0 - pe).abs() < 1e-12 {
        // Complete agreement with a single category: kappa is undefined; the
        // conventional convention is to report 1 when P₀ = 1.
        if (po - 1.0).abs() < 1e-12 {
            1.0
        } else {
            f64::NAN
        }
    } else {
        (po - pe) / (1.0 - pe)
    };
    Kappa {
        n,
        observed: po,
        expected: pe,
        kappa,
    }
}

/// The most frequent rating of a list (the first in lexicographic order on a
/// tie, which keeps the result deterministic).
fn modal(values: &[String]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for value in values {
        *counts.entry(value.as_str()).or_insert(0) += 1;
    }
    let mut best = "";
    let mut best_count = 0;
    for (value, count) in counts {
        if count > best_count {
            best = value;
            best_count = count;
        }
    }
    best.to_owned()
}

/// Runs the agreement analysis.
pub fn fit(df: &DataFrame, part: &str, appraiser: &str, rating: &str) -> Result<Agreement> {
    if part == appraiser || part == rating || appraiser == rating {
        return Err(Error::Unsupported(
            "the part, appraiser and rating must be three different columns".to_owned(),
        ));
    }
    let part_column = df.column(part)?;
    let appraiser_column = df.column(appraiser)?;
    let rating_column = df.column(rating)?;

    // (part, appraiser) → ratings, in row order.
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut parts: Vec<String> = Vec::new();
    let mut appraisers: Vec<String> = Vec::new();
    let mut n = 0usize;
    for row in 0..df.n_rows() {
        let Some(part_label) = label(part_column, row) else {
            continue;
        };
        let Some(appraiser_label) = label(appraiser_column, row) else {
            continue;
        };
        let Some(rating_label) = label(rating_column, row) else {
            continue;
        };
        if !parts.contains(&part_label) {
            parts.push(part_label.clone());
        }
        if !appraisers.contains(&appraiser_label) {
            appraisers.push(appraiser_label.clone());
        }
        groups
            .entry((part_label, appraiser_label))
            .or_default()
            .push(rating_label);
        n += 1;
    }
    if parts.len() < 2 || appraisers.len() < 2 {
        return Err(Error::Unsupported(
            "the analysis needs at least two parts and two appraisers".to_owned(),
        ));
    }

    let mut within = Vec::with_capacity(appraisers.len());
    for appraiser_label in &appraisers {
        let mut first = Vec::new();
        let mut second = Vec::new();
        for part_label in &parts {
            if let Some(values) = groups.get(&(part_label.clone(), appraiser_label.clone()))
                && values.len() >= 2
            {
                first.push(values[0].clone());
                second.push(values[1].clone());
            }
        }
        let used = first.len();
        within.push(WithinAppraiser {
            appraiser: appraiser_label.clone(),
            parts: used,
            kappa: cohen_kappa(&first, &second),
        });
    }

    // Modal rating of each appraiser on each part.
    let mut modal_ratings: BTreeMap<(String, String), String> = BTreeMap::new();
    for ((part_label, appraiser_label), values) in &groups {
        modal_ratings.insert((part_label.clone(), appraiser_label.clone()), modal(values));
    }
    let mut between = Vec::new();
    for (i, first_appraiser) in appraisers.iter().enumerate() {
        for second_appraiser in appraisers.iter().skip(i + 1) {
            let mut first = Vec::new();
            let mut second = Vec::new();
            for part_label in &parts {
                let left = modal_ratings.get(&(part_label.clone(), first_appraiser.clone()));
                let right = modal_ratings.get(&(part_label.clone(), second_appraiser.clone()));
                if let (Some(left), Some(right)) = (left, right) {
                    first.push(left.clone());
                    second.push(right.clone());
                }
            }
            between.push(BetweenAppraisers {
                first: first_appraiser.clone(),
                second: second_appraiser.clone(),
                kappa: cohen_kappa(&first, &second),
            });
        }
    }

    Ok(Agreement {
        parts,
        appraisers,
        n,
        within,
        between,
    })
}

/// Runs the analysis and builds the report.
pub fn analyze(
    df: &DataFrame,
    part: &str,
    appraiser: &str,
    rating: &str,
) -> Result<AnalysisOutput> {
    let model = fit(df, part, appraiser, rating)?;
    let mut result = AnalysisResult::new(
        format!("Attribute Agreement Analysis — {part} × {appraiser} × {rating}"),
        "Cohen's kappa within and between appraisers".to_owned(),
    );
    result.n = model.n;
    result.excluded = df.n_rows() - model.n;

    let mean_within = if model.within.is_empty() {
        f64::NAN
    } else {
        model
            .within
            .iter()
            .map(|entry| entry.kappa.kappa)
            .sum::<f64>()
            / model.within.len() as f64
    };
    result.statistics = vec![
        ("Parts", model.parts.len() as f64),
        ("Appraisers", model.appraisers.len() as f64),
        ("Ratings", model.n as f64),
        ("Mean within kappa", mean_within),
    ];

    result.extra.push(Item::Table {
        headers: vec![
            "Appraiser".to_owned(),
            "Parts".to_owned(),
            "Agreement".to_owned(),
            "Kappa".to_owned(),
            "Expected".to_owned(),
        ],
        rows: model
            .within
            .iter()
            .map(|entry| {
                vec![
                    entry.appraiser.clone(),
                    format!("{}", entry.parts),
                    format!("{:.4}", entry.kappa.observed),
                    format!("{:.4}", entry.kappa.kappa),
                    format!("{:.4}", entry.kappa.expected),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Appraiser 1".to_owned(),
            "Appraiser 2".to_owned(),
            "Parts".to_owned(),
            "Agreement".to_owned(),
            "Kappa".to_owned(),
        ],
        rows: model
            .between
            .iter()
            .map(|entry| {
                vec![
                    entry.first.clone(),
                    entry.second.clone(),
                    format!("{}", entry.kappa.n),
                    format!("{:.4}", entry.kappa.observed),
                    format!("{:.4}", entry.kappa.kappa),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Kappa".to_owned(),
            "kappa = (P₀ − Pₑ)/(1 − Pₑ); one is perfect agreement and zero the agreement \
             expected by chance"
                .to_owned(),
        ),
        (
            "Within appraiser".to_owned(),
            "the first two ratings of each part by the same appraiser are compared".to_owned(),
        ),
        (
            "Between appraisers".to_owned(),
            "each part takes the modal rating of each appraiser, and the pair of appraisers is \
             compared over the parts"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing part, appraiser or rating are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the repetition and modal-rating rules are standard choices and have not been verified \
         against captured output"
            .to_owned(),
    );

    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame(rows: &[(&str, &str, &str)]) -> DataFrame {
        DataFrame::new(vec![
            Column::new_text("part", rows.iter().map(|row| row.0.to_owned()).collect()),
            Column::new_text(
                "appraiser",
                rows.iter().map(|row| row.1.to_owned()).collect(),
            ),
            Column::new_text("rating", rows.iter().map(|row| row.2.to_owned()).collect()),
        ])
        .expect("frame")
    }

    #[test]
    fn perfect_agreement_has_a_kappa_of_one() {
        let df = frame(&[
            ("1", "A", "pass"),
            ("1", "A", "pass"),
            ("2", "A", "fail"),
            ("2", "A", "fail"),
            ("1", "B", "pass"),
            ("1", "B", "pass"),
            ("2", "B", "fail"),
            ("2", "B", "fail"),
        ]);
        let model = fit(&df, "part", "appraiser", "rating").expect("fit");
        assert_eq!(model.within.len(), 2);
        for entry in &model.within {
            assert!((entry.kappa.kappa - 1.0).abs() < 1e-12, "{:?}", entry);
            assert!((entry.kappa.observed - 1.0).abs() < 1e-12);
        }
        assert_eq!(model.between.len(), 1);
        assert!((model.between[0].kappa.kappa - 1.0).abs() < 1e-12);
    }

    #[test]
    fn chance_agreement_has_a_kappa_near_zero() {
        // Each appraiser says pass/fail alternately; the margins are balanced,
        // so the agreement is the one expected by chance.
        let df = frame(&[
            ("1", "A", "pass"),
            ("2", "A", "fail"),
            ("3", "A", "pass"),
            ("4", "A", "fail"),
            ("1", "B", "pass"),
            ("2", "B", "pass"),
            ("3", "B", "fail"),
            ("4", "B", "fail"),
        ]);
        let model = fit(&df, "part", "appraiser", "rating").expect("fit");
        let between = &model.between[0].kappa;
        // Agreement over 4 parts: 2 of 4 match; expected 0.5.
        assert!((between.observed - 0.5).abs() < 1e-12, "{between:?}");
        assert!((between.expected - 0.5).abs() < 1e-12);
        assert!(between.kappa.abs() < 1e-12, "{between:?}");
    }

    #[test]
    fn the_report_is_built_and_missing_rows_are_counted() {
        let df = frame(&[
            ("1", "A", "pass"),
            ("1", "A", ""),
            ("2", "A", "fail"),
            ("2", "A", "fail"),
            ("1", "B", "pass"),
            ("1", "B", "pass"),
            ("2", "B", "fail"),
            ("2", "B", "fail"),
        ]);
        let out = analyze(&df, "part", "appraiser", "rating").expect("analysis");
        assert_eq!(out.results[0].excluded, 1);
        let text = out.results[0].to_session_text();
        assert!(text.contains("Attribute Agreement"), "{text}");
        assert!(text.contains("Kappa"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn the_three_columns_must_be_different() {
        let df = frame(&[("1", "A", "pass")]);
        assert!(matches!(
            fit(&df, "part", "part", "rating"),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            fit(&df, "part", "appraiser", "rating"),
            Err(Error::Unsupported(_))
        ));
    }
}
