//! Crossed Gage R&R by the ANOVA method.
//!
//! Scope: a balanced crossed design with one measurement per part/operator/
//! replicate. The model includes the part×operator interaction. Variance
//! components are estimated from the ANOVA mean squares and negative
//! components are set to zero. The report labels the method and the balanced
//! design requirement.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item};
use crate::special::f_cdf;
use crate::workflow::AnalysisOutput;

/// One variance component.
#[derive(Debug, Clone, PartialEq)]
pub struct VarianceComponent {
    /// Source name.
    pub source: String,
    /// Estimated variance component.
    pub variance: f64,
    /// Percentage of total variance.
    pub contribution: f64,
    /// Percentage of study variation (`6σ` scale).
    pub study_variation: f64,
}

/// Fitted crossed Gage R&R.
#[derive(Debug, Clone, PartialEq)]
pub struct GageRr {
    /// Number of parts.
    pub parts: usize,
    /// Number of operators.
    pub operators: usize,
    /// Replicates per part×operator cell.
    pub replicates: usize,
    /// Observations used.
    pub n: usize,
    /// Rows removed because a measurement, part or operator was missing.
    pub excluded: usize,
    /// Sums of squares.
    pub ss_part: f64,
    /// Operator sum of squares.
    pub ss_operator: f64,
    /// Interaction sum of squares.
    pub ss_interaction: f64,
    /// Error sum of squares.
    pub ss_error: f64,
    /// Total sum of squares.
    pub ss_total: f64,
    /// Degrees of freedom.
    pub df_part: f64,
    /// Operator degrees of freedom.
    pub df_operator: f64,
    /// Interaction degrees of freedom.
    pub df_interaction: f64,
    /// Error degrees of freedom.
    pub df_error: f64,
    /// Mean squares.
    pub ms_part: f64,
    /// Operator mean square.
    pub ms_operator: f64,
    /// Interaction mean square.
    pub ms_interaction: f64,
    /// Error mean square.
    pub ms_error: f64,
    /// Part F statistic.
    pub f_part: f64,
    /// Operator F statistic.
    pub f_operator: f64,
    /// Interaction F statistic.
    pub f_interaction: f64,
    /// Part p-value.
    pub p_part: f64,
    /// Operator p-value.
    pub p_operator: f64,
    /// Interaction p-value.
    pub p_interaction: f64,
    /// Estimated variance components.
    pub components: Vec<VarianceComponent>,
    /// Repeatability standard deviation.
    pub repeatability: f64,
    /// Reproducibility standard deviation.
    pub reproducibility: f64,
    /// Total Gage R&R standard deviation.
    pub grr: f64,
    /// Part-to-part standard deviation.
    pub part_sd: f64,
    /// Total study standard deviation.
    pub total_sd: f64,
    /// %Gage R&R of study variation.
    pub percent_grr: f64,
    /// %Repeatability of study variation.
    pub percent_ev: f64,
    /// %Reproducibility of study variation.
    pub percent_av: f64,
    /// Number of distinct categories.
    pub ndc: f64,
}

fn cell(column: &Column, row: usize) -> Option<f64> {
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

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn p_value(f: f64, d1: f64, d2: f64) -> f64 {
    if f.is_finite() {
        (1.0 - f_cdf(f, d1, d2)).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Fits a crossed Gage R&R.
pub fn fit(df: &DataFrame, measurement: &str, part: &str, operator: &str) -> Result<GageRr> {
    if measurement == part || measurement == operator || part == operator {
        return Err(Error::Unsupported(
            "measurement, part and operator must be different columns".to_owned(),
        ));
    }
    let measurement_column = df.column(measurement)?;
    if matches!(
        measurement_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_)
    ) {
        return Err(Error::Csv(format!(
            "measurement column {measurement} is not numeric"
        )));
    }
    let part_column = df.column(part)?;
    let operator_column = df.column(operator)?;

    let mut cells: BTreeMap<(String, String), Vec<f64>> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let (Some(value), Some(part_label), Some(operator_label)) = (
            cell(measurement_column, row),
            label(part_column, row),
            label(operator_column, row),
        ) else {
            continue;
        };
        cells
            .entry((part_label, operator_label))
            .or_default()
            .push(value);
    }
    if cells.is_empty() {
        return Err(Error::EmptySample);
    }
    let replicates = cells.values().next().map_or(0, Vec::len);
    if replicates == 0 || cells.values().any(|values| values.len() != replicates) {
        return Err(Error::Unsupported(
            "Gage R&R requires a balanced design: every part×operator cell must have the same \
             number of replicates"
                .to_owned(),
        ));
    }
    let parts = {
        let mut names: Vec<&str> = cells.keys().map(|(part, _)| part.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    };
    let operators = {
        let mut names: Vec<&str> = cells
            .keys()
            .map(|(_, operator)| operator.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    };
    if parts < 2 || operators < 2 || replicates < 2 {
        return Err(Error::Unsupported(
            "Gage R&R needs at least two parts, two operators and two replicates".to_owned(),
        ));
    }
    if cells.len() != parts * operators {
        return Err(Error::Unsupported(
            "the design is not fully crossed".to_owned(),
        ));
    }

    let n = cells.values().map(Vec::len).sum::<usize>();
    let grand = cells.values().flat_map(|values| values.iter()).sum::<f64>() / n as f64;
    let part_means: BTreeMap<String, f64> = {
        let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for ((part, _), values) in &cells {
            grouped.entry(part.clone()).or_default().extend(values);
        }
        grouped
            .into_iter()
            .map(|(key, values)| (key, mean(&values)))
            .collect()
    };
    let operator_means: BTreeMap<String, f64> = {
        let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for ((_, operator), values) in &cells {
            grouped.entry(operator.clone()).or_default().extend(values);
        }
        grouped
            .into_iter()
            .map(|(key, values)| (key, mean(&values)))
            .collect()
    };
    let cell_means: BTreeMap<(String, String), f64> = cells
        .iter()
        .map(|(key, values)| (key.clone(), mean(values)))
        .collect();

    let ss_part = operators as f64
        * replicates as f64
        * part_means
            .values()
            .map(|value| (value - grand).powi(2))
            .sum::<f64>();
    let ss_operator = parts as f64
        * replicates as f64
        * operator_means
            .values()
            .map(|value| (value - grand).powi(2))
            .sum::<f64>();
    let ss_cell = replicates as f64
        * cell_means
            .values()
            .map(|value| (value - grand).powi(2))
            .sum::<f64>();
    let ss_interaction = (ss_cell - ss_part - ss_operator).max(0.0);
    let ss_total = cells
        .values()
        .flat_map(|values| values.iter())
        .map(|value| (value - grand).powi(2))
        .sum::<f64>();
    let ss_error = (ss_total - ss_cell).max(0.0);

    let df_part = (parts - 1) as f64;
    let df_operator = (operators - 1) as f64;
    let df_interaction = df_part * df_operator;
    let df_error = parts as f64 * operators as f64 * (replicates as f64 - 1.0);
    let ms_part = ss_part / df_part;
    let ms_operator = ss_operator / df_operator;
    let ms_interaction = if df_interaction > 0.0 {
        ss_interaction / df_interaction
    } else {
        0.0
    };
    let ms_error = if df_error > 0.0 {
        ss_error / df_error
    } else {
        0.0
    };
    let f_part = if ms_interaction > 0.0 {
        ms_part / ms_interaction
    } else {
        f64::INFINITY
    };
    let f_operator = if ms_interaction > 0.0 {
        ms_operator / ms_interaction
    } else {
        f64::INFINITY
    };
    let f_interaction = if ms_error > 0.0 {
        ms_interaction / ms_error
    } else {
        f64::INFINITY
    };
    let p_part = p_value(f_part, df_part, df_interaction);
    let p_operator = p_value(f_operator, df_operator, df_interaction);
    let p_interaction = p_value(f_interaction, df_interaction, df_error);

    let var_repeat = ms_error;
    let var_interaction = ((ms_interaction - ms_error) / replicates as f64).max(0.0);
    let var_operator =
        ((ms_operator - ms_interaction) / (parts as f64 * replicates as f64)).max(0.0);
    let var_part = ((ms_part - ms_interaction) / (operators as f64 * replicates as f64)).max(0.0);
    let total_variance = var_repeat + var_interaction + var_operator + var_part;
    if total_variance <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let percent = |value: f64| 100.0 * value / total_variance;
    let study = |value: f64| 100.0 * value.sqrt() / total_variance.sqrt();
    let components = vec![
        VarianceComponent {
            source: "Repeatability".to_owned(),
            variance: var_repeat,
            contribution: percent(var_repeat),
            study_variation: study(var_repeat),
        },
        VarianceComponent {
            source: "Operator".to_owned(),
            variance: var_operator,
            contribution: percent(var_operator),
            study_variation: study(var_operator),
        },
        VarianceComponent {
            source: "Part×Operator".to_owned(),
            variance: var_interaction,
            contribution: percent(var_interaction),
            study_variation: study(var_interaction),
        },
        VarianceComponent {
            source: "Part".to_owned(),
            variance: var_part,
            contribution: percent(var_part),
            study_variation: study(var_part),
        },
    ];
    let repeatability = var_repeat.sqrt();
    let reproducibility = (var_operator + var_interaction).sqrt();
    let grr = (var_repeat + var_interaction + var_operator).sqrt();
    let part_sd = var_part.sqrt();
    let total_sd = total_variance.sqrt();
    let percent_grr = 100.0 * grr / total_sd;
    let percent_ev = 100.0 * repeatability / total_sd;
    let percent_av = 100.0 * reproducibility / total_sd;
    let ndc = if grr > 0.0 {
        1.41 * part_sd / grr
    } else {
        f64::INFINITY
    };

    Ok(GageRr {
        parts,
        operators,
        replicates,
        n,
        excluded: df.n_rows() - n,
        ss_part,
        ss_operator,
        ss_interaction,
        ss_error,
        ss_total,
        df_part,
        df_operator,
        df_interaction,
        df_error,
        ms_part,
        ms_operator,
        ms_interaction,
        ms_error,
        f_part,
        f_operator,
        f_interaction,
        p_part,
        p_operator,
        p_interaction,
        components,
        repeatability,
        reproducibility,
        grr,
        part_sd,
        total_sd,
        percent_grr,
        percent_ev,
        percent_av,
        ndc,
    })
}

/// Fits Gage R&R and builds the Session report.
pub fn analyze(
    df: &DataFrame,
    measurement: &str,
    part: &str,
    operator: &str,
) -> Result<AnalysisOutput> {
    analyze_design(df, measurement, part, operator, GageDesign::Crossed)
}

/// Fits the requested Gage R&R design and builds the Session report.
pub fn analyze_design(
    df: &DataFrame,
    measurement: &str,
    part: &str,
    operator: &str,
    design: GageDesign,
) -> Result<AnalysisOutput> {
    match design {
        GageDesign::Crossed => {
            let model = fit(df, measurement, part, operator)?;
            Ok(AnalysisOutput {
                results: vec![report(measurement, &model)],
                plot: None,
            })
        }
        GageDesign::Nested => {
            let model = fit_nested(df, measurement, part, operator)?;
            Ok(AnalysisOutput {
                results: vec![report_nested(measurement, &model)],
                plot: None,
            })
        }
    }
}

fn report(measurement: &str, model: &GageRr) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Gage R&R — column {measurement}"),
        "Crossed ANOVA method; balanced design; interaction included",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Parts", model.parts as f64),
        ("Operators", model.operators as f64),
        ("Replicates", model.replicates as f64),
        ("%Gage R&R", model.percent_grr),
        ("%Repeatability", model.percent_ev),
        ("%Reproducibility", model.percent_av),
        ("Number of distinct categories", model.ndc),
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
                "Part".to_owned(),
                format!("{:.0}", model.df_part),
                format!("{:.6}", model.ss_part),
                format!("{:.6}", model.ms_part),
                format!("{:.6}", model.f_part),
                format!("{:.6}", model.p_part),
            ],
            vec![
                "Operator".to_owned(),
                format!("{:.0}", model.df_operator),
                format!("{:.6}", model.ss_operator),
                format!("{:.6}", model.ms_operator),
                format!("{:.6}", model.f_operator),
                format!("{:.6}", model.p_operator),
            ],
            vec![
                "Part×Operator".to_owned(),
                format!("{:.0}", model.df_interaction),
                format!("{:.6}", model.ss_interaction),
                format!("{:.6}", model.ms_interaction),
                format!("{:.6}", model.f_interaction),
                format!("{:.6}", model.p_interaction),
            ],
            vec![
                "Repeatability".to_owned(),
                format!("{:.0}", model.df_error),
                format!("{:.6}", model.ss_error),
                format!("{:.6}", model.ms_error),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!(
                    "{:.0}",
                    model.df_part + model.df_operator + model.df_interaction + model.df_error
                ),
                format!("{:.6}", model.ss_total),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Component".to_owned(),
            "Variance".to_owned(),
            "% Contribution".to_owned(),
            "% Study variation".to_owned(),
        ],
        rows: model
            .components
            .iter()
            .map(|component| {
                vec![
                    component.source.clone(),
                    format!("{:.6}", component.variance),
                    format!("{:.3}", component.contribution),
                    format!("{:.3}", component.study_variation),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "measurement = part + operator + part×operator + error; crossed balanced design"
                .to_owned(),
        ),
        (
            "Variance components".to_owned(),
            "ANOVA mean-square estimators; negative components are set to zero".to_owned(),
        ),
        (
            "Gage R&R".to_owned(),
            "repeatability + reproducibility; %Gage R&R is the ratio of the Gage standard \
             deviation to total study standard deviation"
                .to_owned(),
        ),
        (
            "Distinct categories".to_owned(),
            "1.41·(part SD)/(Gage R&R SD)".to_owned(),
        ),
    ];
    result.warnings.push(
        "Gage R&R conventions, interaction pooling and acceptance thresholds have not been \
         verified against documented output"
            .to_owned(),
    );
    result
}

/// Gage R&R design family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GageDesign {
    /// Every part is measured by every operator.
    #[default]
    Crossed,
    /// Parts are nested within operators (each operator measures its own parts).
    Nested,
}

impl GageDesign {
    /// Name for the report and UI.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Crossed => "crossed",
            Self::Nested => "nested",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "crossed" | "cross" => Some(Self::Crossed),
            "nested" | "nest" => Some(Self::Nested),
            _ => None,
        }
    }
}

/// Nested Gage R&R result.
#[derive(Debug, Clone, PartialEq)]
pub struct NestedGageRr {
    /// Number of operators.
    pub operators: usize,
    /// Parts per operator.
    pub parts_per_operator: usize,
    /// Replicates per part.
    pub replicates: usize,
    /// Observations used.
    pub n: usize,
    /// Rows excluded.
    pub excluded: usize,
    /// Operator sum of squares.
    pub ss_operator: f64,
    /// Part-within-operator sum of squares.
    pub ss_part: f64,
    /// Error sum of squares.
    pub ss_error: f64,
    /// Total sum of squares.
    pub ss_total: f64,
    /// Operator degrees of freedom.
    pub df_operator: f64,
    /// Part degrees of freedom.
    pub df_part: f64,
    /// Error degrees of freedom.
    pub df_error: f64,
    /// Mean squares.
    pub ms_operator: f64,
    /// Part mean square.
    pub ms_part: f64,
    /// Error mean square.
    pub ms_error: f64,
    /// Operator F statistic.
    pub f_operator: f64,
    /// Part F statistic.
    pub f_part: f64,
    /// Operator p-value.
    pub p_operator: f64,
    /// Part p-value.
    pub p_part: f64,
    /// Variance components.
    pub components: Vec<VarianceComponent>,
    /// Repeatability standard deviation.
    pub repeatability: f64,
    /// Reproducibility standard deviation.
    pub reproducibility: f64,
    /// Gage R&R standard deviation.
    pub grr: f64,
    /// Part standard deviation.
    pub part_sd: f64,
    /// Total study standard deviation.
    pub total_sd: f64,
    /// %Gage R&R.
    pub percent_grr: f64,
    /// %Repeatability.
    pub percent_ev: f64,
    /// %Reproducibility.
    pub percent_av: f64,
    /// Number of distinct categories.
    pub ndc: f64,
}

/// Fits a nested Gage R&R.
pub fn fit_nested(
    df: &DataFrame,
    measurement: &str,
    part: &str,
    operator: &str,
) -> Result<NestedGageRr> {
    if measurement == part || measurement == operator || part == operator {
        return Err(Error::Unsupported(
            "measurement, part and operator must be different columns".to_owned(),
        ));
    }
    let measurement_column = df.column(measurement)?;
    if matches!(
        measurement_column.data(),
        ColumnData::Text(_) | ColumnData::Bool(_)
    ) {
        return Err(Error::Csv(format!(
            "measurement column {measurement} is not numeric"
        )));
    }
    let part_column = df.column(part)?;
    let operator_column = df.column(operator)?;

    let mut nested: BTreeMap<String, BTreeMap<String, Vec<f64>>> = BTreeMap::new();
    let mut part_owner: BTreeMap<String, String> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let (Some(value), Some(part_label), Some(operator_label)) = (
            cell(measurement_column, row),
            label(part_column, row),
            label(operator_column, row),
        ) else {
            continue;
        };
        if let Some(owner) = part_owner.get(&part_label) {
            if owner != &operator_label {
                return Err(Error::Unsupported(format!(
                    "part {part_label} appears under more than one operator; use the crossed design"
                )));
            }
        } else {
            part_owner.insert(part_label.clone(), operator_label.clone());
        }
        nested
            .entry(operator_label)
            .or_default()
            .entry(part_label)
            .or_default()
            .push(value);
    }
    if nested.len() < 2 {
        return Err(Error::Unsupported(
            "nested Gage R&R needs at least two operators".to_owned(),
        ));
    }
    let parts_per_operator = nested.values().next().map_or(0, BTreeMap::len);
    let replicates = nested
        .values()
        .next()
        .and_then(|parts| parts.values().next())
        .map_or(0, Vec::len);
    if parts_per_operator < 2 || replicates < 2 {
        return Err(Error::Unsupported(
            "nested Gage R&R needs at least two parts per operator and two replicates".to_owned(),
        ));
    }
    for parts in nested.values() {
        if parts.len() != parts_per_operator {
            return Err(Error::Unsupported(
                "nested Gage R&R requires the same number of parts per operator".to_owned(),
            ));
        }
        for values in parts.values() {
            if values.len() != replicates {
                return Err(Error::Unsupported(
                    "nested Gage R&R requires the same number of replicates per part".to_owned(),
                ));
            }
        }
    }
    let operators = nested.len();
    let n = nested
        .values()
        .flat_map(|parts| parts.values())
        .map(Vec::len)
        .sum::<usize>();
    let grand = nested
        .values()
        .flat_map(|parts| parts.values())
        .flat_map(|values| values.iter())
        .sum::<f64>()
        / n as f64;

    let operator_means: BTreeMap<String, f64> = nested
        .iter()
        .map(|(operator, parts)| {
            let values: Vec<f64> = parts.values().flatten().copied().collect();
            (operator.clone(), mean(&values))
        })
        .collect();
    let part_means: BTreeMap<(String, String), f64> = nested
        .iter()
        .flat_map(|(operator, parts)| {
            parts
                .iter()
                .map(move |(part, values)| ((operator.clone(), part.clone()), mean(values)))
        })
        .collect();

    let ss_operator = parts_per_operator as f64
        * replicates as f64
        * operator_means
            .values()
            .map(|value| (value - grand).powi(2))
            .sum::<f64>();
    let ss_part = replicates as f64
        * part_means
            .iter()
            .map(|((operator, _), value)| {
                let operator_mean = operator_means[operator];
                (value - operator_mean).powi(2)
            })
            .sum::<f64>();
    let ss_total = nested
        .values()
        .flat_map(|parts| parts.values())
        .flat_map(|values| values.iter())
        .map(|value| (value - grand).powi(2))
        .sum::<f64>();
    let ss_error = (ss_total - ss_operator - ss_part).max(0.0);

    let df_operator = (operators - 1) as f64;
    let df_part = (operators * (parts_per_operator - 1)) as f64;
    let df_error = (operators * parts_per_operator * (replicates - 1)) as f64;
    let ms_operator = ss_operator / df_operator;
    let ms_part = ss_part / df_part;
    let ms_error = ss_error / df_error;
    let f_operator = if ms_part > 0.0 {
        ms_operator / ms_part
    } else {
        f64::INFINITY
    };
    let f_part = if ms_error > 0.0 {
        ms_part / ms_error
    } else {
        f64::INFINITY
    };
    let p_operator = p_value(f_operator, df_operator, df_part);
    let p_part = p_value(f_part, df_part, df_error);

    let var_repeat = ms_error;
    let var_part = ((ms_part - ms_error) / replicates as f64).max(0.0);
    let var_operator =
        ((ms_operator - ms_part) / (parts_per_operator as f64 * replicates as f64)).max(0.0);
    let total_variance = var_repeat + var_part + var_operator;
    if total_variance <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let percent = |value: f64| 100.0 * value / total_variance;
    let study = |value: f64| 100.0 * value.sqrt() / total_variance.sqrt();
    let components = vec![
        VarianceComponent {
            source: "Repeatability".to_owned(),
            variance: var_repeat,
            contribution: percent(var_repeat),
            study_variation: study(var_repeat),
        },
        VarianceComponent {
            source: "Operator".to_owned(),
            variance: var_operator,
            contribution: percent(var_operator),
            study_variation: study(var_operator),
        },
        VarianceComponent {
            source: "Part (operator)".to_owned(),
            variance: var_part,
            contribution: percent(var_part),
            study_variation: study(var_part),
        },
    ];
    let repeatability = var_repeat.sqrt();
    let reproducibility = var_operator.sqrt();
    let grr = (var_repeat + var_operator).sqrt();
    let part_sd = var_part.sqrt();
    let total_sd = total_variance.sqrt();
    let ndc = if grr > 0.0 {
        1.41 * part_sd / grr
    } else {
        f64::INFINITY
    };

    Ok(NestedGageRr {
        operators,
        parts_per_operator,
        replicates,
        n,
        excluded: df.n_rows() - n,
        ss_operator,
        ss_part,
        ss_error,
        ss_total,
        df_operator,
        df_part,
        df_error,
        ms_operator,
        ms_part,
        ms_error,
        f_operator,
        f_part,
        p_operator,
        p_part,
        components,
        repeatability,
        reproducibility,
        grr,
        part_sd,
        total_sd,
        percent_grr: 100.0 * grr / total_sd,
        percent_ev: 100.0 * repeatability / total_sd,
        percent_av: 100.0 * reproducibility / total_sd,
        ndc,
    })
}

fn report_nested(measurement: &str, model: &NestedGageRr) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Nested Gage R&R — column {measurement}"),
        "Nested ANOVA; parts nested within operators; balanced design",
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Operators", model.operators as f64),
        ("Parts per operator", model.parts_per_operator as f64),
        ("Replicates", model.replicates as f64),
        ("%Gage R&R", model.percent_grr),
        ("%Repeatability", model.percent_ev),
        ("%Reproducibility", model.percent_av),
        ("Number of distinct categories", model.ndc),
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
                "Operator".to_owned(),
                format!("{:.0}", model.df_operator),
                format!("{:.6}", model.ss_operator),
                format!("{:.6}", model.ms_operator),
                format!("{:.6}", model.f_operator),
                format!("{:.6}", model.p_operator),
            ],
            vec![
                "Part (operator)".to_owned(),
                format!("{:.0}", model.df_part),
                format!("{:.6}", model.ss_part),
                format!("{:.6}", model.ms_part),
                format!("{:.6}", model.f_part),
                format!("{:.6}", model.p_part),
            ],
            vec![
                "Repeatability".to_owned(),
                format!("{:.0}", model.df_error),
                format!("{:.6}", model.ss_error),
                format!("{:.6}", model.ms_error),
                String::new(),
                String::new(),
            ],
            vec![
                "Total".to_owned(),
                format!("{:.0}", model.df_operator + model.df_part + model.df_error),
                format!("{:.6}", model.ss_total),
                String::new(),
                String::new(),
                String::new(),
            ],
        ],
    });
    result.extra.push(Item::Table {
        headers: vec![
            "Component".to_owned(),
            "Variance".to_owned(),
            "% Contribution".to_owned(),
            "% Study variation".to_owned(),
        ],
        rows: model
            .components
            .iter()
            .map(|component| {
                vec![
                    component.source.clone(),
                    format!("{:.6}", component.variance),
                    format!("{:.3}", component.contribution),
                    format!("{:.3}", component.study_variation),
                ]
            })
            .collect(),
    });
    result.notes = vec![
        (
            "Model".to_owned(),
            "measurement = operator + part(operator) + error; each part belongs to one operator"
                .to_owned(),
        ),
        (
            "Variance components".to_owned(),
            "ANOVA mean-square estimators; negative components are set to zero".to_owned(),
        ),
        (
            "Distinct categories".to_owned(),
            "1.41·(part SD)/(Gage R&R SD)".to_owned(),
        ),
    ];
    result.warnings.push(
        "nested Gage R&R conventions and acceptance thresholds have not been verified against \
         documented output"
            .to_owned(),
    );
    result
}
