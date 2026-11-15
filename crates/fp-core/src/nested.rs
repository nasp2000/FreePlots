// SPDX-License-Identifier: MIT
//! Two-level nested (hierarchical) analysis of variance.
//!
//! The model is `y_ijk = µ + A_i + B_j(i) + ε_ijk`, with `a` groups, `b`
//! subgroups inside every group and `n` observations inside every subgroup
//! cell. The design must be **balanced**: the same number of subgroups per
//! group and the same number of observations per cell, which is what makes the
//! variance components estimable in closed form.
//!
//! The sums of squares are
//! `SS_A = b·n Σ (ȳ_i.. − ȳ)²`, `SS_B(A) = n Σ (ȳ_ij. − ȳ_i..)²` and
//! `SS_E = Σ (y − ȳ_ij.)²`, with `a − 1`, `a(b − 1)` and `ab(n − 1)` degrees of
//! freedom. The **hierarchical F tests** are `F_A = MS_A/MS_B(A)` (the
//! subgroup mean square is the error of the group term) and
//! `F_B(A) = MS_B(A)/MS_E`.
//!
//! With both factors random, the **variance components** are
//! `σ² = MS_E`, `σ²_B = (MS_B(A) − MS_E)/n` and
//! `σ²_A = (MS_A − MS_B(A))/(b·n)`; a negative estimate is reported as zero and
//! flagged.

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// Groups, their subgroups and the observations of every cell.
type NestedCells = std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<f64>>>;

/// One subgroup cell of the design.
#[derive(Debug, Clone, PartialEq)]
pub struct NestedCell {
    /// Group label.
    pub group: String,
    /// Subgroup label.
    pub subgroup: String,
    /// Observations in the cell.
    pub n: usize,
    /// Mean of the cell.
    pub mean: f64,
}

/// One variance component.
#[derive(Debug, Clone, PartialEq)]
pub struct VarianceComponent {
    /// Source name.
    pub source: String,
    /// Estimate (a negative estimate is reported as zero).
    pub estimate: f64,
    /// Percentage of the total variance.
    pub percent: f64,
    /// Whether the raw estimate was negative and was set to zero.
    pub clamped: bool,
}

/// Fitted two-level nested design.
#[derive(Debug, Clone, PartialEq)]
pub struct NestedFit {
    /// Group labels, sorted.
    pub groups: Vec<String>,
    /// Subgroup labels of each group, in the order of `groups`.
    pub subgroups: Vec<Vec<String>>,
    /// Cell summaries.
    pub cells: Vec<NestedCell>,
    /// Degrees of freedom of the group term.
    pub df_a: f64,
    /// Degrees of freedom of the subgroup term.
    pub df_b: f64,
    /// Degrees of freedom of the error.
    pub df_e: f64,
    /// Sum of squares of the group term.
    pub ss_a: f64,
    /// Sum of squares of the subgroup term.
    pub ss_b: f64,
    /// Error sum of squares.
    pub ss_e: f64,
    /// Mean square of the group term.
    pub ms_a: f64,
    /// Mean square of the subgroup term.
    pub ms_b: f64,
    /// Error mean square.
    pub ms_e: f64,
    /// F ratio of the group term against the subgroup term.
    pub f_a: f64,
    /// p-value of the group term.
    pub p_a: f64,
    /// F ratio of the subgroup term against the error.
    pub f_b: f64,
    /// p-value of the subgroup term.
    pub p_b: f64,
    /// Variance components (groups, subgroups, error, total).
    pub components: Vec<VarianceComponent>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded for a missing value.
    pub excluded: usize,
}

fn value(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|v| v.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&v| v as f64),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_) => None,
    }
}

fn label(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Date(values) => Some(crate::data::format_date(values[row])),
        ColumnData::I64(values) => values.get(row).map(ToString::to_string),
        ColumnData::F64(values) => values.get(row).map(ToString::to_string),
        ColumnData::Bool(values) => values
            .get(row)
            .map(|&v| if v { "T" } else { "F" }.to_owned()),
    }
}

/// Fits the two-level nested design.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] for the same column twice, a numeric
/// response that is not numeric, fewer than two groups or subgroups, an
/// unbalanced design and a singular error term; the usual column errors
/// otherwise.
pub fn fit(df: &DataFrame, response: &str, group: &str, subgroup: &str) -> Result<NestedFit> {
    if response == group || response == subgroup || group == subgroup {
        return Err(Error::Unsupported(
            "the response, the group and the subgroup must be three different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    if matches!(
        response_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_) | ColumnData::Date(_)
    ) {
        return Err(Error::Csv(format!(
            "response column {response} is not numeric"
        )));
    }
    let group_column = df.column(group)?;
    let subgroup_column = df.column(subgroup)?;

    let mut cells: NestedCells = std::collections::BTreeMap::new();
    let mut excluded = 0usize;
    for row in 0..df.n_rows() {
        let (Some(y), Some(outer), Some(inner)) = (
            value(response_column, row),
            label(group_column, row),
            label(subgroup_column, row),
        ) else {
            excluded += 1;
            continue;
        };
        cells
            .entry(outer)
            .or_default()
            .entry(inner)
            .or_default()
            .push(y);
    }
    if cells.is_empty() {
        return Err(Error::EmptySample);
    }
    let groups: Vec<String> = cells.keys().cloned().collect();
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the design needs at least two groups".to_owned(),
        ));
    }
    let mut subgroups: Vec<Vec<String>> = Vec::with_capacity(groups.len());
    let mut sizes: Vec<usize> = Vec::new();
    for name in &groups {
        let inner = cells.get(name).expect("group was collected");
        let labels: Vec<String> = inner.keys().cloned().collect();
        if labels.len() < 2 {
            return Err(Error::Unsupported(format!(
                "group {name} has a single subgroup: the design needs at least two"
            )));
        }
        subgroups.push(labels);
        for values in inner.values() {
            sizes.push(values.len());
        }
    }
    let b = subgroups[0].len();
    if subgroups.iter().any(|labels| labels.len() != b) {
        return Err(Error::Unsupported(
            "the nested design must be balanced: every group needs the same number of subgroups"
                .to_owned(),
        ));
    }
    let n_cell = sizes[0];
    if sizes.iter().any(|size| *size != n_cell) {
        return Err(Error::Unsupported(
            "the nested design must be balanced: every subgroup needs the same number of \
             observations"
                .to_owned(),
        ));
    }
    if n_cell < 2 {
        return Err(Error::SampleTooSmall { n: n_cell, min: 2 });
    }
    let a = groups.len();
    let total = a * b * n_cell;

    let mut summaries: Vec<NestedCell> = Vec::with_capacity(a * b);
    let mut overall = 0.0;
    for name in &groups {
        for inner_name in cells.get(name).expect("group").keys() {
            let values = cells
                .get(name)
                .and_then(|inner| inner.get(inner_name))
                .expect("cell");
            let mean = values.iter().sum::<f64>() / values.len() as f64;
            overall += values.iter().sum::<f64>();
            summaries.push(NestedCell {
                group: name.clone(),
                subgroup: inner_name.clone(),
                n: values.len(),
                mean,
            });
        }
    }
    overall /= total as f64;

    let mut ss_a = 0.0;
    let mut ss_b = 0.0;
    let mut ss_e = 0.0;
    for name in &groups {
        let inner = cells.get(name).expect("group");
        let group_values: Vec<f64> = inner
            .values()
            .flat_map(|values| values.iter().copied())
            .collect();
        let group_mean = group_values.iter().sum::<f64>() / group_values.len() as f64;
        ss_a += b as f64 * n_cell as f64 * (group_mean - overall).powi(2);
        for values in inner.values() {
            let cell_mean = values.iter().sum::<f64>() / values.len() as f64;
            ss_b += n_cell as f64 * (cell_mean - group_mean).powi(2);
            for value in values {
                ss_e += (value - cell_mean).powi(2);
            }
        }
    }
    let df_a = (a - 1) as f64;
    let df_b = (a * (b - 1)) as f64;
    let df_e = (a * b * (n_cell - 1)) as f64;
    let ms_a = ss_a / df_a;
    let ms_b = ss_b / df_b;
    let ms_e = ss_e / df_e;
    if ms_e <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let f_a = if ms_b > 0.0 { ms_a / ms_b } else { f64::NAN };
    let p_a = (1.0 - f_cdf(f_a, df_a, df_b)).clamp(0.0, 1.0);
    let f_b = ms_b / ms_e;
    let p_b = (1.0 - f_cdf(f_b, df_b, df_e)).clamp(0.0, 1.0);

    let raw_b = (ms_b - ms_e) / n_cell as f64;
    let raw_a = (ms_a - ms_b) / (b as f64 * n_cell as f64);
    let estimates = [ms_e, raw_b.max(0.0), raw_a.max(0.0)];
    let total_variance: f64 = estimates.iter().sum();
    let components = vec![
        VarianceComponent {
            source: format!("{group} (groups)"),
            estimate: estimates[2],
            percent: if total_variance > 0.0 {
                100.0 * estimates[2] / total_variance
            } else {
                f64::NAN
            },
            clamped: raw_a < 0.0,
        },
        VarianceComponent {
            source: format!("{subgroup} ({group})"),
            estimate: estimates[1],
            percent: if total_variance > 0.0 {
                100.0 * estimates[1] / total_variance
            } else {
                f64::NAN
            },
            clamped: raw_b < 0.0,
        },
        VarianceComponent {
            source: "Error".to_owned(),
            estimate: estimates[0],
            percent: if total_variance > 0.0 {
                100.0 * estimates[0] / total_variance
            } else {
                f64::NAN
            },
            clamped: false,
        },
        VarianceComponent {
            source: "Total".to_owned(),
            estimate: total_variance,
            percent: 100.0,
            clamped: false,
        },
    ];

    Ok(NestedFit {
        groups,
        subgroups,
        cells: summaries,
        df_a,
        df_b,
        df_e,
        ss_a,
        ss_b,
        ss_e,
        ms_a,
        ms_b,
        ms_e,
        f_a,
        p_a,
        f_b,
        p_b,
        components,
        n: total,
        excluded,
    })
}

/// Builds the nested ANOVA report.
///
/// # Errors
///
/// The errors of [`fit`].
pub fn analyze(
    df: &DataFrame,
    response: &str,
    group: &str,
    subgroup: &str,
) -> Result<AnalysisOutput> {
    let model = fit(df, response, group, subgroup)?;
    let mut result = AnalysisResult::new(
        format!("Nested ANOVA — response {response}"),
        format!(
            "Two-level nested design: {subgroup} inside {group}; random-effects variance components"
        ),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Groups", model.groups.len() as f64),
        ("Subgroups per group", model.subgroups[0].len() as f64),
        ("Observations per cell", model.cells[0].n as f64),
        ("F (groups)", model.f_a),
        ("p (groups)", model.p_a),
        ("F (subgroups)", model.f_b),
        ("p (subgroups)", model.p_b),
    ];
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: vec![
            vec![
                format!("{group} (groups)"),
                format!("{:.0}", model.df_a),
                format!("{:.6}", model.ss_a),
                format!("{:.6}", model.ms_a),
                format!("{:.6}", model.f_a),
                format!("{:.6}", model.p_a),
            ],
            vec![
                format!("{subgroup} ({group})"),
                format!("{:.0}", model.df_b),
                format!("{:.6}", model.ss_b),
                format!("{:.6}", model.ms_b),
                format!("{:.6}", model.f_b),
                format!("{:.6}", model.p_b),
            ],
            vec![
                "Error".to_owned(),
                format!("{:.0}", model.df_e),
                format!("{:.6}", model.ss_e),
                format!("{:.6}", model.ms_e),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", model.df_a + model.df_b + model.df_e),
                format!("{:.6}", model.ss_a + model.ss_b + model.ss_e),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "Estimate".to_owned(),
            "Percent".to_owned(),
        ],
        rows: model
            .components
            .iter()
            .map(|component| {
                vec![
                    component.source.clone(),
                    format!("{:.6}", component.estimate),
                    format!("{:.2}", component.percent),
                ]
            })
            .collect(),
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Subgroup".to_owned(),
            "Group".to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
        ],
        rows: model
            .cells
            .iter()
            .map(|cell| {
                vec![
                    cell.subgroup.clone(),
                    cell.group.clone(),
                    cell.n.to_string(),
                    format!("{:.6}", cell.mean),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            format!(
                "y = µ + {group} + {subgroup}({group}) + error; the subgroup factor is nested \
                 inside the group factor and the design is balanced"
            ),
        ),
        (
            "Hierarchical F".to_owned(),
            format!(
                "the group term is tested against the subgroup mean square and the subgroup \
                 term against the error: F = MS_{group}/MS_{subgroup}({group}) and \
                 F = MS_{subgroup}({group})/MS_error"
            ),
        ),
        (
            "Variance components".to_owned(),
            "random-effects estimators: σ²(error) = MS_error, σ²(subgroup) = (MS_subgroup − \
             MS_error)/n and σ²(group) = (MS_group − MS_subgroup)/(b·n); a negative estimate is \
             reported as zero"
                .to_owned(),
        ),
        (
            "Scope".to_owned(),
            "two levels and a balanced design; unbalanced designs, more than two levels and \
             confidence intervals for the components are outside this command"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(model.n, model.excluded, 0);
    if model.components.iter().any(|component| component.clamped) {
        result.warnings.push(
            "a variance component was negative and is reported as zero: with few groups the \
             estimate is unstable"
                .to_owned(),
        );
    }
    Ok(AnalysisOutput {
        results: vec![result],
        plot: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(columns: Vec<Column>) -> DataFrame {
        DataFrame::new(columns).expect("frame")
    }

    /// Two groups, two subgroups each and two observations per cell, with cell
    /// means 4, 6, 10 and 12 and a unit spread inside each cell.
    fn nested_frame() -> DataFrame {
        let mut y = Vec::new();
        let mut group = Vec::new();
        let mut subgroup = Vec::new();
        for (outer, inner, centre) in [
            ("A", "a1", 4.0),
            ("A", "a2", 6.0),
            ("B", "b1", 10.0),
            ("B", "b2", 12.0),
        ] {
            for offset in [-1.0, 1.0] {
                y.push(centre + offset);
                group.push(outer.to_owned());
                subgroup.push(inner.to_owned());
            }
        }
        frame(vec![
            Column::new_f64("y", y),
            Column::new_text("g", group),
            Column::new_text("s", subgroup),
        ])
    }

    #[test]
    fn the_nested_design_decomposes_the_sums_of_squares() {
        let df = nested_frame();
        let model = fit(&df, "y", "g", "s").expect("nested");
        assert_eq!(model.n, 8);
        assert_eq!(model.df_a, 1.0);
        assert_eq!(model.df_b, 2.0);
        assert_eq!(model.df_e, 4.0);
        assert!((model.ss_a - 72.0).abs() < 1e-9, "{}", model.ss_a);
        assert!((model.ss_b - 8.0).abs() < 1e-9, "{}", model.ss_b);
        assert!((model.ss_e - 8.0).abs() < 1e-9, "{}", model.ss_e);
        assert!((model.ms_a - 72.0).abs() < 1e-9);
        assert!((model.ms_b - 4.0).abs() < 1e-9);
        assert!((model.ms_e - 2.0).abs() < 1e-9);
        // Hierarchical tests: groups against subgroups, subgroups against error.
        assert!((model.f_a - 18.0).abs() < 1e-9, "{}", model.f_a);
        assert!((model.p_a - 0.051_316).abs() < 1e-5, "{}", model.p_a);
        assert!((model.f_b - 2.0).abs() < 1e-9);
        assert!((model.p_b - 0.25).abs() < 1e-9, "{}", model.p_b);
        // Components: 2, (4 − 2)/2 = 1 and (72 − 4)/4 = 17, total 20.
        let components = &model.components;
        assert!((components[2].estimate - 2.0).abs() < 1e-9);
        assert!((components[1].estimate - 1.0).abs() < 1e-9);
        assert!((components[0].estimate - 17.0).abs() < 1e-9);
        assert!((components[3].estimate - 20.0).abs() < 1e-9);
        assert!(
            (components[0].percent - 85.0).abs() < 1e-9,
            "{}",
            components[0].percent
        );
        assert!((components[1].percent - 5.0).abs() < 1e-9);
        assert!((components[2].percent - 10.0).abs() < 1e-9);
        let text = analyze(&df, "y", "g", "s")
            .expect("report")
            .to_session_text();
        assert!(text.contains("Nested ANOVA — response y"), "{text}");
        assert!(text.contains("s (g)"), "{text}");
        assert!(text.contains("Total"), "{text}");
    }

    #[test]
    fn the_nested_design_flags_a_zero_component() {
        // The subgroups differ by less than the error, so the subgroup
        // component is negative and is reported as zero.
        let df = frame(vec![
            Column::new_f64("y", vec![0.0, 2.0, 2.0, 0.0, 10.0, 12.0, 10.0, 12.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "A", "B", "B", "B", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
            Column::new_text(
                "s",
                ["a1", "a1", "a2", "a2", "b1", "b1", "b2", "b2"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        let model = fit(&df, "y", "g", "s").expect("nested");
        assert!(model.components[1].clamped, "{:?}", model.components[1]);
        assert!(model.components[1].estimate == 0.0);
        let text = analyze(&df, "y", "g", "s")
            .expect("report")
            .to_session_text();
        assert!(text.contains("negative"), "{text}");
    }

    #[test]
    fn the_nested_design_rejects_invalid_requests() {
        let df = nested_frame();
        // Unbalanced: one group has an extra subgroup.
        let mut y: Vec<f64> = (0..8).map(|index| index as f64).collect();
        let mut g: Vec<String> = vec!["A".to_owned(); 4];
        g.extend(vec!["B".to_owned(); 4]);
        let mut s: Vec<String> = vec!["a1".to_owned(); 2];
        s.extend(vec!["a2".to_owned(); 2]);
        s.extend(vec!["b1".to_owned(); 2]);
        s.extend(vec!["b2".to_owned(); 2]);
        y.push(9.0);
        y.push(10.0);
        g.push("B".to_owned());
        g.push("B".to_owned());
        s.push("b3".to_owned());
        s.push("b3".to_owned());
        let frame_unbalanced = frame(vec![
            Column::new_f64("y", y),
            Column::new_text("g", g),
            Column::new_text("s", s),
        ]);
        assert!(
            fit(&frame_unbalanced, "y", "g", "s")
                .unwrap_err()
                .to_string()
                .contains("balanced")
        );
        // The same column twice and a single group.
        assert!(fit(&df, "y", "g", "g").is_err());
        let single = frame(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "g",
                ["A", "A", "A", "A"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
            Column::new_text(
                "s",
                ["a1", "a1", "a2", "a2"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(fit(&single, "y", "g", "s").is_err());
        // A constant response has no error variance.
        let flat = frame(vec![
            Column::new_f64("y", vec![2.0; 8]),
            Column::new_text(
                "g",
                ["A", "A", "A", "A", "B", "B", "B", "B"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
            Column::new_text(
                "s",
                ["a1", "a1", "a2", "a2", "b1", "b1", "b2", "b2"]
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            ),
        ]);
        assert!(matches!(
            fit(&flat, "y", "g", "s"),
            Err(Error::ConstantSample)
        ));
    }
}
