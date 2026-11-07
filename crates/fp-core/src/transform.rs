// SPDX-License-Identifier: MIT
//! Column transformations behind the Calc menu.
//!
//! Each function adds a column (or a set of columns) to the worksheet; the
//! formulas live here, never in the interface.

use crate::data::{Column, DType, DataFrame};
use crate::error::{Error, Result};
use crate::special::{ln_gamma, normal_cdf, normal_pdf, normal_quantile, regularized_gamma_upper};

/// Statistic computed by the Column and Row Statistics dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnStat {
    /// Sum of the values.
    Sum,
    /// Arithmetic mean.
    Mean,
    /// Sample standard deviation (`n − 1`).
    Sd,
    /// Smallest value.
    Min,
    /// Largest value.
    Max,
    /// Median (interpolated).
    Median,
    /// Number of non-missing values.
    Count,
}

impl ColumnStat {
    /// Parses the name used by the interface.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "sum" => Some(Self::Sum),
            "mean" => Some(Self::Mean),
            "sd" | "stdev" | "standard deviation" => Some(Self::Sd),
            "min" | "minimum" => Some(Self::Min),
            "max" | "maximum" => Some(Self::Max),
            "median" => Some(Self::Median),
            "count" | "n" => Some(Self::Count),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sum => "Sum",
            Self::Mean => "Mean",
            Self::Sd => "Standard deviation",
            Self::Min => "Minimum",
            Self::Max => "Maximum",
            Self::Median => "Median",
            Self::Count => "Count",
        }
    }
}

/// Computes one statistic over the finite values.
///
/// # Errors
///
/// Returns [`Error::EmptySample`] when no value is finite and
/// [`Error::InvalidParameter`] for the standard deviation of one value.
pub fn column_stat(values: &[f64], stat: ColumnStat) -> Result<f64> {
    let mut finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if stat == ColumnStat::Count {
        return Ok(finite.len() as f64);
    }
    if finite.is_empty() {
        return Err(Error::EmptySample);
    }
    let value = match stat {
        ColumnStat::Sum => finite.iter().sum(),
        ColumnStat::Mean => finite.iter().sum::<f64>() / finite.len() as f64,
        ColumnStat::Sd => {
            if finite.len() < 2 {
                return Err(Error::InvalidParameter {
                    name: "standard deviation sample size",
                    value: finite.len() as f64,
                });
            }
            let mean = finite.iter().sum::<f64>() / finite.len() as f64;
            let ss: f64 = finite.iter().map(|v| (v - mean) * (v - mean)).sum();
            (ss / (finite.len() - 1) as f64).sqrt()
        }
        ColumnStat::Min => finite.iter().copied().fold(f64::INFINITY, f64::min),
        ColumnStat::Max => finite.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ColumnStat::Median => {
            finite.sort_by(f64::total_cmp);
            let n = finite.len();
            if n % 2 == 1 {
                finite[n / 2]
            } else {
                0.5 * (finite[n / 2 - 1] + finite[n / 2])
            }
        }
        ColumnStat::Count => unreachable!(),
    };
    Ok(value)
}

/// Adds one row per selected column holding its statistic.
///
/// # Errors
///
/// As [`column_stat`], plus [`Error::Unsupported`] when the name is empty or
/// already used.
pub fn column_statistics(
    df: &DataFrame,
    columns: &[String],
    stat: ColumnStat,
) -> Result<Vec<(String, f64)>> {
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    let mut values = Vec::with_capacity(columns.len());
    for column in columns {
        let data = df.column(column)?;
        let numeric = data.numeric(crate::data::MissingPolicy::Drop)?;
        values.push((column.clone(), column_stat(&numeric, stat)?));
    }
    Ok(values)
}

/// Adds a column with one statistic per row, computed across `columns`.
///
/// # Errors
///
/// As [`column_stat`], plus [`Error::Unsupported`] for an empty name.
pub fn add_row_statistics(
    df: &mut DataFrame,
    columns: &[String],
    stat: ColumnStat,
    name: &str,
) -> Result<()> {
    check_new_name(df, name)?;
    if columns.is_empty() {
        return Err(Error::Unsupported("select at least one column".to_owned()));
    }
    let mut values = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let row_values: Vec<f64> = columns
            .iter()
            .filter_map(|name| df.column(name).ok()?.value(row))
            .collect();
        values.push(column_stat(&row_values, stat).unwrap_or(f64::NAN));
    }
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))?;
    Ok(())
}

/// Simple set of numbers: `from`, `from + step`, … up to `to`.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a zero step or a direction that
/// never reaches `to`.
pub fn patterned_simple(from: f64, to: f64, step: f64) -> Result<Vec<f64>> {
    if !step.is_finite() || step == 0.0 || !from.is_finite() || !to.is_finite() {
        return Err(Error::InvalidParameter {
            name: "patterned-data step",
            value: step,
        });
    }
    if (to - from) * step < 0.0 {
        return Err(Error::InvalidParameter {
            name: "patterned-data direction",
            value: step,
        });
    }
    let count = ((to - from) / step).floor() as usize + 1;
    if count > 1_000_000 {
        return Err(Error::InvalidParameter {
            name: "patterned-data length",
            value: count as f64,
        });
    }
    Ok((0..count).map(|i| from + i as f64 * step).collect())
}

/// Parses a comma-separated list of numbers.
///
/// # Errors
///
/// Returns [`Error::Csv`] for a value that is not a number.
pub fn parse_number_list(text: &str) -> Result<Vec<f64>> {
    let mut values = Vec::new();
    for part in text.split([',', ';', ' ']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let value: f64 = part
            .replace(',', ".")
            .parse()
            .map_err(|_| Error::Csv(format!("invalid number: {part}")))?;
        values.push(value);
    }
    if values.is_empty() {
        return Err(Error::Csv("the list is empty".to_owned()));
    }
    Ok(values)
}

/// Adds a numeric column with the given values.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] when the name is empty or already used.
pub fn add_values(df: &mut DataFrame, name: &str, values: Vec<f64>) -> Result<()> {
    check_new_name(df, name)?;
    let rows = df.n_rows().max(values.len());
    df.ensure_rows(rows)?;
    let mut values = values;
    values.resize(rows, f64::NAN);
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))?;
    Ok(())
}

/// Adds one 0/1 indicator column per level of a text column.
///
/// Returns the number of columns added.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] when the column has no levels.
pub fn add_indicators(df: &mut DataFrame, column: &str) -> Result<usize> {
    let rows = df.n_rows();
    let mut levels: Vec<String> = Vec::new();
    {
        let source = df.column(column)?;
        for row in 0..rows {
            if let Some(label) = source.label(row)
                && !levels.contains(&label)
            {
                levels.push(label);
            }
        }
    }
    if levels.is_empty() {
        return Err(Error::Unsupported("the column has no levels".to_owned()));
    }
    let mut additions = Vec::with_capacity(levels.len());
    for level in &levels {
        let values: Vec<f64> = {
            let source = df.column(column)?;
            (0..rows)
                .map(|row| {
                    if source.label(row).as_deref() == Some(level.as_str()) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect()
        };
        additions.push((format!("{column}_{}", sanitise(level)), values));
    }
    for (name, values) in additions {
        let index = df.n_columns();
        df.insert_column(index, Column::new_f64(&name, values))?;
    }
    Ok(levels.len())
}

/// Part of a date extracted by the Calc menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePart {
    /// Calendar year.
    Year,
    /// Month (1–12).
    Month,
    /// Day of the month.
    Day,
    /// Day of the week (1 = Monday).
    Weekday,
}

impl DatePart {
    /// Parses the name used by the interface.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            "day" => Some(Self::Day),
            "weekday" => Some(Self::Weekday),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Year => "Year",
            Self::Month => "Month",
            Self::Day => "Day",
            Self::Weekday => "Weekday",
        }
    }
}

/// Adds a numeric column with one part of a date column.
///
/// # Errors
///
/// Returns [`Error::Csv`] when the column is not a date column, plus
/// [`Error::Unsupported`] for an empty or repeated name.
pub fn add_date_part(df: &mut DataFrame, column: &str, part: DatePart, name: &str) -> Result<()> {
    check_new_name(df, name)?;
    let source = df.column(column)?;
    if source.dtype() != DType::Date {
        return Err(Error::Csv(format!("{column} is not a date column")));
    }
    let mut values = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        match source.date(row) {
            Some(days) => values.push(date_part(days, part)),
            None => values.push(f64::NAN),
        }
    }
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))?;
    Ok(())
}

fn date_part(days: i64, part: DatePart) -> f64 {
    let (year, month, day) = civil_from_days(days);
    match part {
        DatePart::Year => year as f64,
        DatePart::Month => month as f64,
        DatePart::Day => day as f64,
        DatePart::Weekday => {
            // 1970-01-01 was a Thursday; 1 = Monday.
            ((days.rem_euclid(7) + 3) % 7 + 1) as f64
        }
    }
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Adds `n` normally distributed values with a reproducible seed.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for a negative scale, plus
/// [`Error::Unsupported`] for an empty or repeated name.
pub fn add_random_normal(
    df: &mut DataFrame,
    n: usize,
    mean: f64,
    sd: f64,
    seed: u64,
    name: &str,
) -> Result<()> {
    check_new_name(df, name)?;
    if !sd.is_finite() || sd <= 0.0 || !mean.is_finite() {
        return Err(Error::InvalidParameter {
            name: "normal scale",
            value: sd,
        });
    }
    let mut generator = crate::special::SplitMix64::new(seed);
    let mut values = Vec::with_capacity(n);
    while values.len() < n {
        let u1 = generator.next_f64().max(1e-15);
        let u2 = generator.next_f64();
        let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        values.push(mean + sd * z);
    }
    add_values(df, name, values)
}

/// Adds `n` uniform values on `[low, high)` with a reproducible seed.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for an invalid interval, plus
/// [`Error::Unsupported`] for an empty or repeated name.
pub fn add_random_uniform(
    df: &mut DataFrame,
    n: usize,
    low: f64,
    high: f64,
    seed: u64,
    name: &str,
) -> Result<()> {
    check_new_name(df, name)?;
    if !high.is_finite() || !low.is_finite() || high <= low {
        return Err(Error::InvalidParameter {
            name: "uniform interval",
            value: high,
        });
    }
    let mut generator = crate::special::SplitMix64::new(seed);
    let values = (0..n)
        .map(|_| low + (high - low) * generator.next_f64())
        .collect();
    add_values(df, name, values)
}

/// Distribution of the probability dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProbabilityDistribution {
    /// Normal with mean and standard deviation.
    Normal { mean: f64, sd: f64 },
    /// Binomial with `n` trials and success probability `p`.
    Binomial { trials: u64, p: f64 },
    /// Poisson with mean `lambda`.
    Poisson { lambda: f64 },
}

/// Function of the probability dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbabilityFunction {
    /// Cumulative probability.
    Cdf,
    /// Density or probability mass.
    Pdf,
    /// Inverse cumulative probability.
    Inverse,
}

impl ProbabilityFunction {
    /// Parses the name used by the interface.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "cdf" | "cumulative" => Some(Self::Cdf),
            "pdf" | "density" | "pmf" => Some(Self::Pdf),
            "inverse" | "inv" => Some(Self::Inverse),
            _ => None,
        }
    }

    /// Name used in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Cdf => "CDF",
            Self::Pdf => "PDF",
            Self::Inverse => "Inverse CDF",
        }
    }
}

/// Adds a column with the chosen function of the distribution applied to an
/// input column.
///
/// # Errors
///
/// Returns [`Error::InvalidParameter`] for invalid parameters, plus
/// [`Error::Unsupported`] for an empty or repeated name.
pub fn add_probability_column(
    df: &mut DataFrame,
    input: &str,
    distribution: ProbabilityDistribution,
    function: ProbabilityFunction,
    name: &str,
) -> Result<()> {
    check_new_name(df, name)?;
    let source = df.column(input)?;
    let numeric = source.numeric(crate::data::MissingPolicy::Drop)?;
    validate_distribution(distribution)?;
    let values = numeric
        .iter()
        .map(|&x| probability_value(distribution, function, x))
        .collect();
    let index = df.n_columns();
    df.insert_column(index, Column::new_f64(name, values))?;
    Ok(())
}

fn validate_distribution(distribution: ProbabilityDistribution) -> Result<()> {
    match distribution {
        ProbabilityDistribution::Normal { mean, sd } => {
            if !mean.is_finite() || !sd.is_finite() || sd <= 0.0 {
                return Err(Error::InvalidParameter {
                    name: "normal scale",
                    value: sd,
                });
            }
        }
        ProbabilityDistribution::Binomial { p, .. } => {
            if !(0.0..=1.0).contains(&p) || !p.is_finite() {
                return Err(Error::InvalidParameter {
                    name: "binomial probability",
                    value: p,
                });
            }
        }
        ProbabilityDistribution::Poisson { lambda } => {
            if !lambda.is_finite() || lambda <= 0.0 {
                return Err(Error::InvalidParameter {
                    name: "poisson mean",
                    value: lambda,
                });
            }
        }
    }
    Ok(())
}

fn probability_value(
    distribution: ProbabilityDistribution,
    function: ProbabilityFunction,
    x: f64,
) -> f64 {
    match distribution {
        ProbabilityDistribution::Normal { mean, sd } => {
            let z = (x - mean) / sd;
            match function {
                ProbabilityFunction::Cdf => normal_cdf(z),
                ProbabilityFunction::Pdf => normal_pdf(z) / sd,
                ProbabilityFunction::Inverse => mean + sd * normal_quantile(x),
            }
        }
        ProbabilityDistribution::Binomial { trials, p } => {
            let k = x.floor();
            match function {
                ProbabilityFunction::Cdf => {
                    if k < 0.0 {
                        0.0
                    } else if k >= trials as f64 {
                        1.0
                    } else {
                        binomial_cdf(k as u64, trials, p)
                    }
                }
                ProbabilityFunction::Pdf => {
                    if k < 0.0 || k > trials as f64 {
                        0.0
                    } else {
                        binomial_pmf(k as u64, trials, p)
                    }
                }
                ProbabilityFunction::Inverse => binomial_inverse(x, trials, p),
            }
        }
        ProbabilityDistribution::Poisson { lambda } => {
            let k = x.floor();
            match function {
                ProbabilityFunction::Cdf => {
                    if k < 0.0 {
                        0.0
                    } else {
                        regularized_gamma_upper(k + 1.0, lambda)
                    }
                }
                ProbabilityFunction::Pdf => {
                    if k < 0.0 {
                        0.0
                    } else {
                        (-lambda + k * lambda.ln() - ln_gamma(k + 1.0)).exp()
                    }
                }
                ProbabilityFunction::Inverse => poisson_inverse(x, lambda),
            }
        }
    }
}

fn binomial_pmf(k: u64, n: u64, p: f64) -> f64 {
    if p <= 0.0 {
        return if k == 0 { 1.0 } else { 0.0 };
    }
    if p >= 1.0 {
        return if k == n { 1.0 } else { 0.0 };
    }
    let kf = k as f64;
    let nf = n as f64;
    (ln_gamma(nf + 1.0) - ln_gamma(kf + 1.0) - ln_gamma(nf - kf + 1.0)
        + kf * p.ln()
        + (nf - kf) * (1.0 - p).ln())
    .exp()
}

fn binomial_cdf(k: u64, n: u64, p: f64) -> f64 {
    (0..=k)
        .map(|i| binomial_pmf(i, n, p))
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

fn binomial_inverse(q: f64, n: u64, p: f64) -> f64 {
    let mut cumulative = 0.0;
    for k in 0..=n {
        cumulative += binomial_pmf(k, n, p);
        if cumulative >= q {
            return k as f64;
        }
    }
    n as f64
}

fn poisson_inverse(q: f64, lambda: f64) -> f64 {
    let mut cumulative = 0.0;
    for k in 0..100_000 {
        cumulative += (-lambda + k as f64 * lambda.ln() - ln_gamma(k as f64 + 1.0)).exp();
        if cumulative >= q {
            return k as f64;
        }
    }
    f64::NAN
}

fn check_new_name(df: &DataFrame, name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::Unsupported("name the new column".to_owned()));
    }
    if df.column(name).is_ok() {
        return Err(Error::Unsupported(format!("column {name} already exists")));
    }
    Ok(())
}

fn sanitise(level: &str) -> String {
    level
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0]),
            Column::new_f64("b", vec![10.0, 20.0, 30.0, 40.0]),
        ])
        .expect("frame")
    }

    #[test]
    fn the_column_statistics_match_the_definitions() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(column_stat(&values, ColumnStat::Sum).unwrap(), 10.0);
        assert_eq!(column_stat(&values, ColumnStat::Mean).unwrap(), 2.5);
        assert!(
            (column_stat(&values, ColumnStat::Sd).unwrap() - 1.290_994_448_735_86).abs() < 1e-12
        );
        assert_eq!(column_stat(&values, ColumnStat::Min).unwrap(), 1.0);
        assert_eq!(column_stat(&values, ColumnStat::Max).unwrap(), 4.0);
        assert_eq!(column_stat(&values, ColumnStat::Median).unwrap(), 2.5);
        assert_eq!(column_stat(&values, ColumnStat::Count).unwrap(), 4.0);
    }

    #[test]
    fn the_column_and_row_statistics_are_added_as_columns() {
        let mut df = frame();
        let means = column_statistics(&df, &["a".into(), "b".into()], ColumnStat::Mean)
            .expect("column statistics");
        assert_eq!(means[0], ("a".to_owned(), 2.5));
        assert_eq!(means[1], ("b".to_owned(), 25.0));
        add_row_statistics(&mut df, &["a".into(), "b".into()], ColumnStat::Sum, "total")
            .expect("row statistics");
        assert_eq!(df.column("total").expect("total").value(0), Some(11.0));
        assert_eq!(df.column("total").expect("total").value(3), Some(44.0));
    }

    #[test]
    fn the_patterned_data_walks_from_to_by_step() {
        assert_eq!(
            patterned_simple(1.0, 5.0, 2.0).unwrap(),
            vec![1.0, 3.0, 5.0]
        );
        assert_eq!(patterned_simple(0.0, 1.0, 0.5).unwrap().len(), 3);
        assert!(patterned_simple(0.0, 1.0, 0.0).is_err());
        assert!(patterned_simple(0.0, 1.0, -1.0).is_err());
    }

    #[test]
    fn the_indicators_are_added_per_level() {
        let mut df = DataFrame::new(vec![Column::new_text(
            "group",
            vec!["a".into(), "b".into(), "a".into()],
        )])
        .expect("frame");
        assert_eq!(add_indicators(&mut df, "group").expect("indicators"), 2);
        assert_eq!(df.column("group_a").expect("a").value(0), Some(1.0));
        assert_eq!(df.column("group_a").expect("a").value(1), Some(0.0));
        assert_eq!(df.column("group_b").expect("b").value(1), Some(1.0));
    }

    #[test]
    fn the_date_parts_are_extracted() {
        let mut df = DataFrame::new(vec![Column::new_date(
            "day",
            vec![
                crate::data::parse_date("2024-02-29").expect("date"),
                crate::data::parse_date("2024-03-04").expect("date"),
            ],
        )])
        .expect("frame");
        add_date_part(&mut df, "day", DatePart::Year, "year").expect("year");
        add_date_part(&mut df, "day", DatePart::Month, "month").expect("month");
        add_date_part(&mut df, "day", DatePart::Day, "dom").expect("day");
        add_date_part(&mut df, "day", DatePart::Weekday, "dow").expect("weekday");
        assert_eq!(df.column("year").expect("year").value(0), Some(2024.0));
        assert_eq!(df.column("month").expect("month").value(0), Some(2.0));
        assert_eq!(df.column("dom").expect("dom").value(0), Some(29.0));
        // 2024-02-29 was a Thursday (4) and 2024-03-04 a Monday (1).
        assert_eq!(df.column("dow").expect("dow").value(0), Some(4.0));
        assert_eq!(df.column("dow").expect("dow").value(1), Some(1.0));
    }

    #[test]
    fn the_random_columns_are_reproducible() {
        let mut df = frame();
        add_random_normal(&mut df, 5, 10.0, 2.0, 42, "normal").expect("normal");
        add_random_uniform(&mut df, 5, 0.0, 1.0, 42, "uniform").expect("uniform");
        assert_eq!(df.column("normal").expect("normal").value(0), {
            let mut again = frame();
            add_random_normal(&mut again, 5, 10.0, 2.0, 42, "normal").expect("again");
            again.column("normal").expect("normal").value(0)
        });
        assert!(df.column("uniform").expect("uniform").value(0).unwrap() < 1.0);
    }

    #[test]
    fn the_probability_functions_match_the_closed_forms() {
        let normal = ProbabilityDistribution::Normal { mean: 0.0, sd: 1.0 };
        assert!((probability_value(normal, ProbabilityFunction::Cdf, 0.0) - 0.5).abs() < 1e-12);
        assert!(
            (probability_value(normal, ProbabilityFunction::Inverse, 0.975) - 1.959_964).abs()
                < 1e-4
        );
        let binomial = ProbabilityDistribution::Binomial { trials: 10, p: 0.5 };
        assert!(
            (probability_value(binomial, ProbabilityFunction::Cdf, 5.0) - 0.623_046_875).abs()
                < 1e-9
        );
        assert!(
            (probability_value(binomial, ProbabilityFunction::Pdf, 0.0) - 0.000_976_562_5).abs()
                < 1e-12
        );
        let poisson = ProbabilityDistribution::Poisson { lambda: 2.0 };
        assert!(
            (probability_value(poisson, ProbabilityFunction::Cdf, 2.0) - 0.676_676_2).abs() < 1e-6
        );
        assert!(
            (probability_value(poisson, ProbabilityFunction::Pdf, 0.0) - 0.135_335_28).abs() < 1e-8
        );
    }
}
