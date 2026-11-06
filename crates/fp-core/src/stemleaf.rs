//! Stem-and-leaf display.
//!
//! The values are split into a **stem** (the leading digits) and a **leaf** (the
//! next digit). The display keeps every observation, so unlike a histogram it
//! loses no information: the shape of the distribution and the individual
//! values are visible at the same time.
//!
//! The leaf unit is chosen so that the display has between 5 and 20 stems; the
//! report states the unit and shows the stem, its leaves and the number of
//! observations on the stem.
//!
//! The documented behaviour has a stem-and-leaf display; its unit and rounding
//! rules were not documented, so the numbers are not claimed as documented behaviour
//! — see `the project notes`.

use std::collections::BTreeMap;

use crate::data::{DataFrame, MissingPolicy};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// One line of the display.
#[derive(Debug, Clone, PartialEq)]
pub struct Stem {
    /// Stem value (already multiplied by the leaf unit).
    pub stem: f64,
    /// Leaves, in ascending order.
    pub leaves: Vec<u8>,
}

/// Result of the display.
#[derive(Debug, Clone, PartialEq)]
pub struct StemLeaf {
    /// Stems, from the smallest to the largest.
    pub stems: Vec<Stem>,
    /// Leaf unit (the value of one leaf).
    pub unit: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded (missing or non-finite).
    pub excluded: usize,
    /// Median.
    pub median: f64,
    /// Smallest value.
    pub min: f64,
    /// Largest value.
    pub max: f64,
}

/// Chooses a leaf unit that gives between 5 and 20 stems.
fn leaf_unit(min: f64, max: f64) -> f64 {
    let range = (max - min).abs();
    if range <= 0.0 {
        return 1.0;
    }
    // The unit is a power of ten times 1, 2 or 5.
    let target = range / 10.0;
    let exponent = target.log10().floor();
    let base = 10f64.powf(exponent);
    let multiplier = if target / base < 2.0 {
        1.0
    } else if target / base < 5.0 {
        2.0
    } else {
        5.0
    };
    let unit = multiplier * base;
    if unit > 0.0 { unit } else { 1.0 }
}

/// Builds the display of a sample.
pub fn from_values(values: &[f64]) -> Result<StemLeaf> {
    if values.is_empty() {
        return Err(Error::EmptySample);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let unit = leaf_unit(min, max);
    // One leaf is a tenth of the unit; the stem is the multiple of the unit.
    let leaf = unit / 10.0;
    let mut buckets: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    for value in &sorted {
        let position = (value / leaf).round();
        let stem_index = (position / 10.0).floor() as i64;
        let leaf_value = (position - stem_index as f64 * 10.0).round();
        let leaf_value = leaf_value.clamp(0.0, 9.0) as u8;
        buckets.entry(stem_index).or_default().push(leaf_value);
    }
    let stems: Vec<Stem> = buckets
        .into_iter()
        .map(|(index, mut leaves)| {
            leaves.sort_unstable();
            Stem {
                stem: index as f64 * unit,
                leaves,
            }
        })
        .collect();
    let n = sorted.len();
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    Ok(StemLeaf {
        stems,
        unit,
        n,
        excluded: 0,
        median,
        min,
        max,
    })
}

/// Runs the display and builds the report.
pub fn analyze(df: &DataFrame, column_name: &str) -> Result<AnalysisOutput> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let mut display = from_values(sample.values())?;
    display.excluded = sample.excluded();
    let mut result = AnalysisResult::new(
        format!("Stem-and-Leaf Display — column {column_name}"),
        format!("Leaf unit = {:.6}", display.unit),
    );
    result.n = display.n;
    result.excluded = display.excluded;
    result.statistics = vec![
        ("Leaf unit", display.unit),
        ("Stems", display.stems.len() as f64),
        ("Median", display.median),
        ("Minimum", display.min),
        ("Maximum", display.max),
    ];
    result.extra.push(Item::Table {
        headers: vec!["Stem".to_owned(), "Leaves".to_owned(), "Count".to_owned()],
        rows: display
            .stems
            .iter()
            .map(|stem| {
                vec![
                    format!("{:.6}", stem.stem),
                    stem.leaves
                        .iter()
                        .map(|leaf| leaf.to_string())
                        .collect::<Vec<String>>()
                        .join(" "),
                    format!("{}", stem.leaves.len()),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Reading".to_owned(),
            "a leaf is one tenth of the leaf unit; the value is stem + leaf·unit/10".to_owned(),
        ),
        (
            "Unit".to_owned(),
            "chosen to give between five and twenty stems".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, sample.ties());
    result.warnings.push(
        "the leaf unit and the rounding are standard choices and have not been verified against \
         documented output"
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

    #[test]
    fn the_leaves_keep_every_value() {
        // Values from 1.2 to 3.9 with unit 1: stems 1, 2, 3 and one leaf each.
        let values = vec![1.2, 1.5, 2.0, 2.7, 3.1, 3.9];
        let display = from_values(&values).expect("display");
        assert!(display.unit > 0.0);
        assert!(
            display.stems.len() >= 5 && display.stems.len() <= 21,
            "{} stems",
            display.stems.len()
        );
        let total: usize = display.stems.iter().map(|stem| stem.leaves.len()).sum();
        assert_eq!(total, values.len());
        // Every leaf reconstructs one of the values.
        let leaf = display.unit / 10.0;
        let mut reconstructed: Vec<f64> = display
            .stems
            .iter()
            .flat_map(|stem| {
                stem.leaves
                    .iter()
                    .map(move |value| stem.stem + f64::from(*value) * leaf)
            })
            .collect();
        reconstructed.sort_by(f64::total_cmp);
        for (left, right) in reconstructed.iter().zip(&values) {
            assert!(
                (left - right).abs() <= leaf / 2.0 + 1e-9,
                "{left} vs {right}"
            );
        }
        assert!((display.median - 2.35).abs() < 1e-12);
        assert!((display.min - 1.2).abs() < 1e-12);
        assert!((display.max - 3.9).abs() < 1e-12);
    }

    #[test]
    fn a_wide_range_gets_a_larger_unit() {
        let values: Vec<f64> = (0..50).map(|index| index as f64 * 20.0).collect();
        let display = from_values(&values).expect("display");
        assert!(display.unit >= 20.0, "unit {}", display.unit);
        assert!(display.stems.len() >= 5 && display.stems.len() <= 21);
        let total: usize = display.stems.iter().map(|stem| stem.leaves.len()).sum();
        assert_eq!(total, 50);
    }

    #[test]
    fn the_report_shows_the_display() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![1.2, 1.5, 2.0, 2.7, 3.1, f64::NAN],
        )])
        .expect("frame");
        let out = analyze(&df, "x").expect("analysis");
        assert_eq!(out.results[0].excluded, 1);
        let text = out.results[0].to_session_text();
        assert!(text.contains("Stem-and-Leaf"), "{text}");
        assert!(text.contains("Leaf unit"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn an_empty_sample_is_rejected() {
        assert!(matches!(from_values(&[]), Err(Error::EmptySample)));
    }
}
