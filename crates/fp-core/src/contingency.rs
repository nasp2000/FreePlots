//! Chi-square test for independence of two categorical variables.
//!
//! The counts of the two factors are cross-tabulated; the statistic is
//! `X² = Σ (Oᵢⱼ − Eᵢⱼ)²/Eᵢⱼ` with the expected counts
//! `Eᵢⱼ = rowᵢ·colⱼ/n`, and the degrees of freedom are
//! `(rows − 1)·(cols − 1)`. Cramér's V is reported as the effect size.
//!
//! The documented behaviour has a cross-tabulation and a chi-square test of
//! independence; the conventions were not documented from it, so the numbers
//! are not claimed as documented behaviour — see `the project notes`.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame};
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::chi_square_cdf;

/// Fisher's exact test of a 2×2 table.
///
/// The table is conditioned on its margins, so each possible count of the
/// top-left cell follows the hypergeometric law
/// `P(x) = C(r₁,x)·C(r₂,c₁−x)/C(n,c₁)`. The two-sided p-value adds the
/// probabilities of every table that is no more likely than the observed
/// one (with a 10⁻⁹ tolerance for the ties of the floating-point product).
#[derive(Debug, Clone, PartialEq)]
pub struct FisherExact {
    /// Two-sided p-value.
    pub two_sided: f64,
    /// One-sided p-value for the upper tail (association in the observed
    /// direction).
    pub upper: f64,
    /// One-sided p-value for the lower tail.
    pub lower: f64,
    /// Odds ratio of the observed table (`ad/bc`, `∞` with a zero cell).
    pub odds_ratio: f64,
}

/// Log of `C(n, k)` through `ln_gamma`, stable for the table margins.
fn ln_binomial(n: f64, k: f64) -> f64 {
    crate::special::ln_gamma(n + 1.0)
        - crate::special::ln_gamma(k + 1.0)
        - crate::special::ln_gamma(n - k + 1.0)
}

/// Fisher's exact test of a 2×2 table `[[a, b], [c, d]]`.
pub fn fisher_exact(table: [[f64; 2]; 2]) -> Result<FisherExact> {
    let [[a, b], [c, d]] = table;
    for value in [a, b, c, d] {
        if !value.is_finite() || value < 0.0 || (value - value.round()).abs() > 1e-9 {
            return Err(Error::Unsupported(
                "Fisher's exact test needs non-negative integer counts".to_owned(),
            ));
        }
    }
    let r1 = a + b;
    let r2 = c + d;
    let c1 = a + c;
    let n = r1 + r2;
    if r1 <= 0.0 || r2 <= 0.0 || c1 <= 0.0 || c1 >= n {
        return Err(Error::EmptySample);
    }
    // The top-left cell runs from max(0, c1 − r2) to min(r1, c1).
    let low = (c1 - r2).max(0.0);
    let high = r1.min(c1);
    let log_denominator = ln_binomial(n, c1);
    let probability =
        |x: f64| (ln_binomial(r1, x) + ln_binomial(r2, c1 - x) - log_denominator).exp();
    let observed_log = ln_binomial(r1, a) + ln_binomial(r2, c1 - a) - log_denominator;
    let observed = observed_log.exp();

    let mut two_sided = 0.0;
    let mut upper = 0.0;
    let mut lower = 0.0;
    let mut x = low;
    while x <= high + 1e-9 {
        let p = probability(x);
        if p <= observed * (1.0 + 1e-9) {
            two_sided += p;
        }
        if x >= a {
            upper += p;
        }
        if x <= a {
            lower += p;
        }
        x += 1.0;
    }
    // Guard against the accumulated floating-point error of long tables.
    two_sided = two_sided.clamp(0.0, 1.0);
    upper = upper.clamp(0.0, 1.0);
    lower = lower.clamp(0.0, 1.0);
    let odds_ratio = if b * c > 0.0 {
        a * d / (b * c)
    } else {
        f64::INFINITY
    };
    Ok(FisherExact {
        two_sided,
        upper,
        lower,
        odds_ratio,
    })
}

/// Cross-tabulation of two categorical variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Contingency {
    /// Levels of the row variable, in lexicographic order.
    pub rows: Vec<String>,
    /// Levels of the column variable, in lexicographic order.
    pub columns: Vec<String>,
    /// Observed counts, indexed by `[row][column]`.
    pub observed: Vec<Vec<f64>>,
    /// Expected counts under independence.
    pub expected: Vec<Vec<f64>>,
    /// Chi-square statistic.
    pub statistic: f64,
    /// Degrees of freedom.
    pub degrees_of_freedom: usize,
    /// p-value of the test.
    pub p_value: f64,
    /// Cramér's V.
    cramers_v: f64,
    /// Fisher's exact test (2×2 tables only).
    pub fisher: Option<FisherExact>,
    /// Observations used.
    pub n: usize,
    /// Rows excluded because a value or a label was missing.
    pub excluded: usize,
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

/// Cross-tabulates the two columns and runs the test.
pub fn test(df: &DataFrame, row_column: &str, column_name: &str) -> Result<Contingency> {
    if row_column == column_name {
        return Err(Error::Unsupported(
            "the two variables must be different columns".to_owned(),
        ));
    }
    let row_source = df.column(row_column)?;
    let column_source = df.column(column_name)?;

    let mut counts: BTreeMap<(String, String), f64> = BTreeMap::new();
    let mut excluded = 0;
    for row in 0..df.n_rows() {
        match (label(row_source, row), label(column_source, row)) {
            (Some(row_label), Some(column_label)) => {
                *counts.entry((row_label, column_label)).or_insert(0.0) += 1.0;
            }
            _ => excluded += 1,
        }
    }
    if counts.is_empty() {
        return Err(Error::EmptySample);
    }
    let rows: Vec<String> = counts
        .keys()
        .map(|(row, _)| row.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let columns: Vec<String> = counts
        .keys()
        .map(|(_, column)| column.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if rows.len() < 2 || columns.len() < 2 {
        return Err(Error::Unsupported(
            "the test needs at least two levels in each variable".to_owned(),
        ));
    }

    let mut observed = vec![vec![0.0; columns.len()]; rows.len()];
    for ((row_label, column_label), count) in &counts {
        let i = rows.iter().position(|r| r == row_label).unwrap_or(0);
        let j = columns.iter().position(|c| c == column_label).unwrap_or(0);
        observed[i][j] = *count;
    }
    let n: f64 = observed.iter().flatten().sum();
    let row_totals: Vec<f64> = observed.iter().map(|row| row.iter().sum()).collect();
    let column_totals: Vec<f64> = (0..columns.len())
        .map(|j| observed.iter().map(|row| row[j]).sum())
        .collect();

    let mut expected = vec![vec![0.0; columns.len()]; rows.len()];
    let mut statistic = 0.0;
    for (i, row) in expected.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = row_totals[i] * column_totals[j] / n;
            if *cell > 0.0 {
                let difference = observed[i][j] - *cell;
                statistic += difference * difference / *cell;
            }
        }
    }
    let degrees_of_freedom = (rows.len() - 1) * (columns.len() - 1);
    let p_value = (1.0 - chi_square_cdf(statistic, degrees_of_freedom as f64)).clamp(0.0, 1.0);
    let smallest = (rows.len().min(columns.len()) - 1) as f64;
    let cramers_v = if smallest > 0.0 && n > 0.0 {
        (statistic / (n * smallest)).sqrt().clamp(0.0, 1.0)
    } else {
        f64::NAN
    };

    let fisher = if rows.len() == 2 && columns.len() == 2 {
        Some(fisher_exact([
            [observed[0][0], observed[0][1]],
            [observed[1][0], observed[1][1]],
        ])?)
    } else {
        None
    };

    Ok(Contingency {
        rows,
        columns,
        observed,
        expected,
        statistic,
        degrees_of_freedom,
        p_value,
        cramers_v,
        fisher,
        n: n as usize,
        excluded,
    })
}

/// Builds the Session report of the chi-square test.
pub fn report(row_column: &str, column_name: &str, model: &Contingency) -> AnalysisResult {
    let mut result = AnalysisResult::new(
        format!("Chi-Square Test for Independence — {row_column} × {column_name}"),
        "Cross-tabulation with the chi-square statistic of independence".to_owned(),
    );
    result.n = model.n;
    result.excluded = model.excluded;
    result.statistics = vec![
        ("Chi-square", model.statistic),
        ("df", model.degrees_of_freedom as f64),
        ("p-value", model.p_value),
        ("Cramer's V", model.cramers_v),
    ];
    if let Some(fisher) = &model.fisher {
        result
            .statistics
            .push(("Fisher exact p (two-sided)", fisher.two_sided));
        result
            .statistics
            .push(("Fisher exact p (lower)", fisher.lower));
        result
            .statistics
            .push(("Fisher exact p (upper)", fisher.upper));
        result.statistics.push(("Odds ratio", fisher.odds_ratio));
    }

    let mut headers = vec![row_column.to_owned()];
    headers.extend(model.columns.iter().cloned());
    headers.push("Total".to_owned());
    let mut rows = Vec::with_capacity(model.rows.len() + 1);
    for (i, level) in model.rows.iter().enumerate() {
        let mut row = vec![level.clone()];
        for j in 0..model.columns.len() {
            row.push(format!(
                "{:.0} ({:.2})",
                model.observed[i][j], model.expected[i][j]
            ));
        }
        let total: f64 = model.observed[i].iter().sum();
        row.push(format!("{total:.0}"));
        rows.push(row);
    }
    let mut totals = vec!["Total".to_owned()];
    for j in 0..model.columns.len() {
        let total: f64 = model.observed.iter().map(|row| row[j]).sum();
        totals.push(format!("{total:.0}"));
    }
    let grand: f64 = model.observed.iter().flatten().sum();
    totals.push(format!("{grand:.0}"));
    rows.push(totals);
    result.extra.push(Item::Table { headers, rows });
    result.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the two variables are independent; H₁: they are associated".to_owned(),
        ),
        (
            "Cells".to_owned(),
            "the counts in brackets are the expected counts under independence".to_owned(),
        ),
        (
            "Exact test".to_owned(),
            if model.fisher.is_some() {
                "for 2×2 tables Fisher's exact test conditions on the margins and adds the \
                 probability of every table no more likely than the observed one"
            } else {
                "Fisher's exact test is only reported for 2×2 tables"
            }
            .to_owned(),
        ),
        (
            "Effect size".to_owned(),
            "Cramer's V is the square root of the chi-square divided by n and the smaller \\
             dimension minus one"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing label in either variable are excluded and counted".to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the cross-tabulation conventions are standard choices; they have not been verified \
         against documented output"
            .to_owned(),
    );
    result
}
