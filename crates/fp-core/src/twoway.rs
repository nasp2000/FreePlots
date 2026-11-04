//! Two-way ANOVA with a balanced design.
//!
//! The design must be **balanced**: every cell of the two factors holds the
//! same number of observations. The sums of squares follow the classical
//! decomposition
//!
//! ```text
//! SS_total = SS_A + SS_B + SS_AB + SS_error
//! ```
//!
//! with `SS_A = b·n·Σ(ȳᵢ· − ȳ)²`, `SS_B = a·n·Σ(ȳ·ⱼ − ȳ)²` and
//! `SS_AB = n·Σ(ȳᵢⱼ − ȳᵢ· − ȳ·ⱼ + ȳ)²`. An unbalanced design is refused
//! instead of being analysed with a convention that was not verified.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::plot::{PlotSpec, Point, Segment, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::f_cdf;

/// Cell of the design: the level of each factor and the observations.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Level of the first factor.
    pub first: String,
    /// Level of the second factor.
    pub second: String,
    /// Observations of the cell.
    pub values: Vec<f64>,
}

impl Cell {
    /// Number of observations.
    pub fn n(&self) -> usize {
        self.values.len()
    }

    /// Mean of the cell.
    pub fn mean(&self) -> f64 {
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }
}

/// Outcome of the two-way ANOVA.
#[derive(Debug, Clone, PartialEq)]
pub struct TwoWayFit {
    /// Cells of the design, in lexicographic order.
    pub cells: Vec<Cell>,
    /// Levels of the first factor.
    pub levels_first: Vec<String>,
    /// Levels of the second factor.
    pub levels_second: Vec<String>,
    /// Grand mean of all observations.
    pub grand_mean: f64,
    /// Sum of squares of the first factor.
    pub ss_first: f64,
    /// Sum of squares of the second factor.
    pub ss_second: f64,
    /// `true` when the interaction term was fitted.
    pub interaction: bool,
    /// Degrees of freedom of the error.
    pub df_error: f64,
    /// Sum of squares of the interaction.
    pub ss_interaction: f64,
    /// Sum of squares of the error.
    pub ss_error: f64,
    /// Total sum of squares.
    pub ss_total: f64,
    /// F statistic of the first factor.
    pub f_first: f64,
    /// p-value of the first factor.
    pub p_first: f64,
    /// F statistic of the second factor.
    pub f_second: f64,
    /// p-value of the second factor.
    pub p_second: f64,
    /// F statistic of the interaction.
    pub f_interaction: f64,
    /// p-value of the interaction.
    pub p_interaction: f64,
    /// Residual standard error.
    pub pooled_sd: f64,
    /// Coefficient of determination.
    pub r_squared: f64,
    /// Observations used.
    pub n: usize,
    /// Rows excluded because a value or a label was missing.
    pub excluded: usize,
}

fn cell_value(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied().filter(|value| value.is_finite()),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
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
            .map(|&value| if value { "T" } else { "F" }.to_owned()),
    }
}

/// Fits the two-way ANOVA.
pub fn fit(df: &DataFrame, response: &str, first: &str, second: &str) -> Result<TwoWayFit> {
    fit_with(df, response, first, second, true)
}

/// Fits the two-way model, optionally without the interaction.
///
/// # Errors
///
/// As [`fit`].
pub fn fit_with(
    df: &DataFrame,
    response: &str,
    first: &str,
    second: &str,
    interaction: bool,
) -> Result<TwoWayFit> {
    if response == first || response == second || first == second {
        return Err(Error::Unsupported(
            "the response and the two factors must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let first_column = df.column(first)?;
    let second_column = df.column(second)?;

    let mut grouped: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    let mut excluded = 0;
    for row in 0..df.n_rows() {
        let value = cell_value(response_column, row);
        let first_label = label(first_column, row);
        let second_label = label(second_column, row);
        match (value, first_label, second_label) {
            (Some(value), Some(first_label), Some(second_label)) => {
                grouped
                    .entry((first_label, second_label))
                    .or_default()
                    .push(value);
            }
            _ => excluded += 1,
        }
    }
    if grouped.len() < 2 {
        return Err(Error::Unsupported(
            "the design needs at least two cells".to_owned(),
        ));
    }
    let levels_first: Vec<String> = grouped
        .keys()
        .map(|(a, _)| a.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let levels_second: Vec<String> = grouped
        .keys()
        .map(|(_, b)| b.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let a = levels_first.len();
    let b = levels_second.len();
    if a < 2 || b < 2 {
        return Err(Error::Unsupported(
            "each factor needs at least two levels".to_owned(),
        ));
    }

    // The design must be complete and balanced.
    let mut per_cell = None;
    for values in grouped.values() {
        if per_cell.is_none() {
            per_cell = Some(values.len());
        } else if per_cell != Some(values.len()) {
            return Err(Error::Unsupported(
                "the design must be balanced: the cells hold different numbers of observations"
                    .to_owned(),
            ));
        }
    }
    let n = per_cell.unwrap_or(0);
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    if grouped.len() != a * b {
        return Err(Error::Unsupported(
            "the design is incomplete: some cells of the two factors are empty".to_owned(),
        ));
    }

    let cells: Vec<Cell> = grouped
        .into_iter()
        .map(|((first, second), values)| Cell {
            first,
            second,
            values,
        })
        .collect();

    let total_n = cells.len() * n;
    let grand_mean: f64 = cells
        .iter()
        .flat_map(|cell| cell.values.iter().copied())
        .sum::<f64>()
        / total_n as f64;

    // Marginal means.
    let mut mean_first = BTreeMap::new();
    for level in &levels_first {
        let values: Vec<f64> = cells
            .iter()
            .filter(|cell| &cell.first == level)
            .flat_map(|cell| cell.values.iter().copied())
            .collect();
        mean_first.insert(
            level.clone(),
            values.iter().sum::<f64>() / values.len() as f64,
        );
    }
    let mut mean_second = BTreeMap::new();
    for level in &levels_second {
        let values: Vec<f64> = cells
            .iter()
            .filter(|cell| &cell.second == level)
            .flat_map(|cell| cell.values.iter().copied())
            .collect();
        mean_second.insert(
            level.clone(),
            values.iter().sum::<f64>() / values.len() as f64,
        );
    }

    let mut ss_first = 0.0;
    for mean in mean_first.values() {
        ss_first += b as f64 * n as f64 * (mean - grand_mean).powi(2);
    }
    let mut ss_second = 0.0;
    for mean in mean_second.values() {
        ss_second += a as f64 * n as f64 * (mean - grand_mean).powi(2);
    }
    let mut ss_interaction = 0.0;
    let mut ss_error = 0.0;
    let mut ss_total = 0.0;
    for cell in &cells {
        let cell_mean = cell.mean();
        let expected = mean_first[&cell.first] + mean_second[&cell.second] - grand_mean;
        ss_interaction += n as f64 * (cell_mean - expected).powi(2);
        for value in &cell.values {
            ss_error += (value - cell_mean).powi(2);
            ss_total += (value - grand_mean).powi(2);
        }
    }

    let df_first = (a - 1) as f64;
    let df_second = (b - 1) as f64;
    let df_interaction = df_first * df_second;
    // The captured command fits main effects only: the interaction's degrees
    // of freedom go to the error and SS_error = SS_total - SS_A - SS_B.
    let (df_error, ss_error) = if interaction {
        ((a * b * (n - 1)) as f64, ss_error)
    } else {
        (
            total_n as f64 - a as f64 - b as f64 + 1.0,
            ss_total - ss_first - ss_second,
        )
    };
    let ms_error = ss_error / df_error;
    let f_first = (ss_first / df_first) / ms_error;
    let f_second = (ss_second / df_second) / ms_error;
    let f_interaction = if interaction {
        (ss_interaction / df_interaction) / ms_error
    } else {
        f64::NAN
    };
    let p_first = (1.0 - f_cdf(f_first, df_first, df_error)).clamp(0.0, 1.0);
    let p_second = (1.0 - f_cdf(f_second, df_second, df_error)).clamp(0.0, 1.0);
    let p_interaction = if interaction {
        (1.0 - f_cdf(f_interaction, df_interaction, df_error)).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };
    let pooled_sd = ms_error.max(0.0).sqrt();
    let r_squared = if ss_total > 0.0 {
        (1.0 - ss_error / ss_total).clamp(0.0, 1.0)
    } else {
        f64::NAN
    };

    Ok(TwoWayFit {
        cells,
        levels_first,
        levels_second,
        interaction,
        df_error,
        grand_mean,
        ss_first,
        ss_second,
        ss_interaction,
        ss_error,
        ss_total,
        f_first,
        p_first,
        f_second,
        p_second,
        f_interaction,
        p_interaction,
        pooled_sd,
        r_squared,
        n: total_n,
        excluded,
    })
}

/// Builds the report of the two-way ANOVA.
pub fn report(response: &str, first: &str, second: &str, model: &TwoWayFit) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Two-Way ANOVA — {response} by {first} and {second}"),
        if model.interaction {
            "Balanced two-way ANOVA with the interaction".to_owned()
        } else {
            "Balanced two-way ANOVA, main effects only (the captured convention)".to_owned()
        },
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Grand mean", model.grand_mean),
        ("Pooled standard deviation", model.pooled_sd),
        ("R²", model.r_squared),
        ("F (factor 1)", model.f_first),
        ("p-value (factor 1)", model.p_first),
        ("F (factor 2)", model.f_second),
        ("p-value (factor 2)", model.p_second),
    ];
    if model.interaction {
        result
            .statistics
            .push(("F (interaction)", model.f_interaction));
        result
            .statistics
            .push(("p-value (interaction)", model.p_interaction));
    }
    result.extra.push(Item::Table {
        headers: vec![
            "Source".to_owned(),
            "DF".to_owned(),
            "SS".to_owned(),
            "MS".to_owned(),
            "F".to_owned(),
            "P".to_owned(),
        ],
        rows: {
            let mut rows = vec![
                source_row(
                    first,
                    model.ss_first,
                    (model.levels_first.len() - 1) as f64,
                    model.f_first,
                    model.p_first,
                ),
                source_row(
                    second,
                    model.ss_second,
                    (model.levels_second.len() - 1) as f64,
                    model.f_second,
                    model.p_second,
                ),
            ];
            if model.interaction {
                rows.push(source_row(
                    "Interaction",
                    model.ss_interaction,
                    ((model.levels_first.len() - 1) * (model.levels_second.len() - 1)) as f64,
                    model.f_interaction,
                    model.p_interaction,
                ));
            }
            rows.push(source_row(
                "Error",
                model.ss_error,
                model.df_error,
                f64::NAN,
                f64::NAN,
            ));
            rows.push(source_row(
                "Total",
                model.ss_total,
                model.n as f64 - 1.0,
                f64::NAN,
                f64::NAN,
            ));
            rows
        },
    });
    result.extra.push(Item::Table {
        headers: vec![
            first.to_owned(),
            second.to_owned(),
            "n".to_owned(),
            "Mean".to_owned(),
        ],
        rows: model
            .cells
            .iter()
            .map(|cell| {
                vec![
                    cell.first.clone(),
                    cell.second.clone(),
                    cell.n().to_string(),
                    format!("{:.6}", cell.mean()),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the effect of each factor and of the interaction is zero".to_owned(),
        ),
        (
            "Design".to_owned(),
            "the design must be complete and balanced; an unbalanced design is refused".to_owned(),
        ),
        (
            "Sums of squares".to_owned(),
            "the classical decomposition SS_total = SS_A + SS_B + SS_AB + SS_error".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value or a missing label are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the two-way conventions are standard choices; they have not been verified against \
         captured output"
            .to_owned(),
    );
    result
}

/// One row of the ANOVA table.
fn source_row(name: &str, ss: f64, df: f64, f: f64, p: f64) -> Vec<String> {
    let ms = if df > 0.0 { ss / df } else { f64::NAN };
    vec![
        name.to_owned(),
        format!("{df:.0}"),
        format!("{ss:.6}"),
        format!("{ms:.6}"),
        if f.is_finite() {
            format!("{f:.6}")
        } else {
            String::new()
        },
        if p.is_finite() {
            format!("{p:.6}")
        } else {
            String::new()
        },
    ]
}

/// Builds the residual-versus-fitted plot of the two-way ANOVA.
pub fn residual_plot(response: &str, model: &TwoWayFit) -> PlotSpec {
    let mut points = Vec::new();
    let mut residuals = Vec::new();
    let mut fitted = Vec::new();
    for cell in &model.cells {
        let cell_mean = cell.mean();
        for value in &cell.values {
            points.push(Point {
                x: cell_mean,
                y: *value,
            });
            fitted.push(cell_mean);
            residuals.push(value - cell_mean);
        }
    }
    let y_min = residuals.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = residuals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_min = fitted.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = fitted.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_pad = 0.05 * (x_max - x_min).abs().max(f64::MIN_POSITIVE);
    let y_pad = 0.05 * (y_max - y_min).abs().max(f64::MIN_POSITIVE);
    let zero = if y_min <= 0.0 && y_max >= 0.0 {
        Some(Segment {
            from: Point {
                x: x_min - x_pad,
                y: 0.0,
            },
            to: Point {
                x: x_max + x_pad,
                y: 0.0,
            },
            width: 1.0,
        })
    } else {
        None
    };
    let mut lines = Vec::new();
    if let Some(zero) = zero {
        lines.push(zero);
    }
    PlotSpec {
        title: format!("Residuals vs Fitted — {response}"),
        x_axis: linear_axis("Fitted", x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis("Residual", y_min - y_pad, y_max + y_pad),
        size: (900, 620),
        points: Vec::new(),
        lines,
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 1,
        secondary: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        // Two factors with two levels each and three observations per cell;
        // the first factor shifts the response by one.
        let mut rows = Vec::new();
        for a in ["a1", "a2"] {
            for b in ["b1", "b2"] {
                for k in 0..3 {
                    let base = if a == "a1" { 10.0 } else { 13.0 };
                    let value = base + if b == "b2" { 1.0 } else { 0.0 } + k as f64 * 0.01;
                    rows.push((a.to_owned(), b.to_owned(), value));
                }
            }
        }
        DataFrame::new(vec![
            Column::new_f64("y", rows.iter().map(|(_, _, v)| *v).collect()),
            Column::new_text("a", rows.iter().map(|(a, _, _)| a.clone()).collect()),
            Column::new_text("b", rows.iter().map(|(_, b, _)| b.clone()).collect()),
        ])
        .expect("frame")
    }

    #[test]
    fn the_two_factors_are_detected() {
        let df = frame();
        let model = fit(&df, "y", "a", "b").expect("fit");
        assert_eq!(model.levels_first, vec!["a1", "a2"]);
        assert_eq!(model.levels_second, vec!["b1", "b2"]);
        assert_eq!(model.cells.len(), 4);
        assert_eq!(model.n, 12);
        // The first factor shifts the response by three, the second by one.
        assert!(model.p_first < 0.001, "p {}", model.p_first);
        assert!(model.p_second < 0.05, "p {}", model.p_second);
        // The interaction is null: the shift does not depend on the level.
        assert!(model.p_interaction > 0.5, "p {}", model.p_interaction);
    }

    #[test]
    fn the_sums_of_squares_add_up() {
        let df = frame();
        let model = fit(&df, "y", "a", "b").expect("fit");
        let sum = model.ss_first + model.ss_second + model.ss_interaction + model.ss_error;
        assert!(
            (sum - model.ss_total).abs() < 1e-9,
            "{sum} vs {}",
            model.ss_total
        );
    }

    #[test]
    fn an_unbalanced_design_is_refused() {
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_text(
                "a",
                vec![
                    "a1".to_owned(),
                    "a1".to_owned(),
                    "a2".to_owned(),
                    "a2".to_owned(),
                ],
            ),
            Column::new_text(
                "b",
                vec![
                    "b1".to_owned(),
                    "b1".to_owned(),
                    "b1".to_owned(),
                    "b2".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        assert!(matches!(
            fit(&df, "y", "a", "b"),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn the_report_lists_the_cells() {
        let df = frame();
        let model = fit(&df, "y", "a", "b").expect("fit");
        let text = report("y", "a", "b", &model).to_session_text();
        assert!(text.contains("Two-Way ANOVA"), "{text}");
        assert!(text.contains("Interaction"), "{text}");
        assert!(text.contains("a1"), "{text}");
    }
}
