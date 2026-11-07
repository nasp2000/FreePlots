// SPDX-License-Identifier: MIT
//! Non-parametric rank tests: Mann–Whitney, Wilcoxon signed-rank and
//! Kruskal–Wallis.
//!
//! All three work on the ranks of the observations, so they need no normal
//! assumption; the p-values use the normal approximation (with a continuity
//! correction and a tie correction) or the chi-square distribution. The
//! documented behaviour offers the same tests, but the conventions were not
//! documented from it, so the numbers are not claimed as documented behaviour —
//! see `the project notes`.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DataFrame, MissingPolicy, Sample};
use crate::error::{Error, Result};
use crate::hypothesis::Alternative;
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::special::{chi_square_cdf, normal_cdf};

/// Mid-ranks of a sample, with the average rank for tied values.
///
/// Ranks are one-based; a tie group occupying the positions `first … last`
/// receives the rank `(first + last + 1) / 2`.
pub(crate) fn mid_ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[end]] == values[order[start]] {
            end += 1;
        }
        // The group occupies the one-based ranks start+1 … end.
        let mid = (start + 1 + end) as f64 / 2.0;
        for index in start..end {
            ranks[order[index]] = mid;
        }
        start = end;
    }
    ranks
}

/// Sum of `t³ − t` over the tie groups, the tie correction of the rank tests.
fn tie_term(values: &[f64]) -> f64 {
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut total = 0.0;
    let mut start = 0;
    while start < sorted.len() {
        let mut end = start + 1;
        while end < sorted.len() && sorted[end] == sorted[start] {
            end += 1;
        }
        let t = (end - start) as f64;
        total += t * t * t - t;
        start = end;
    }
    total
}

/// Outcome of the Mann–Whitney test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MannWhitney {
    /// Size of the first sample.
    pub n1: usize,
    /// Size of the second sample.
    pub n2: usize,
    /// Statistic of the first sample (the number of wins).
    pub u1: f64,
    /// Statistic of the second sample.
    pub u2: f64,
    /// Rank sum of the first sample.
    pub rank_sum1: f64,
    /// Standardised statistic (normal approximation, continuity corrected).
    pub z: f64,
    /// p-value of the test.
    pub p_value: f64,
    /// Hodges–Lehmann estimate of the shift (median of the pairwise differences).
    pub hodges_lehmann: f64,
    /// Tie correction applied to the variance.
    pub tie_correction: f64,
}

/// Outcome of the Wilcoxon signed-rank test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WilcoxonSignedRank {
    /// Number of non-zero differences used.
    pub n: usize,
    /// Sum of the ranks of the positive differences.
    pub w_plus: f64,
    /// Sum of the ranks of the negative differences.
    pub w_minus: f64,
    /// Standardised statistic (normal approximation, continuity corrected).
    pub z: f64,
    /// p-value of the test.
    pub p_value: f64,
    /// Median of the differences.
    pub median_difference: f64,
    /// Tie correction applied to the variance.
    pub tie_correction: f64,
}

/// Outcome of the Kruskal–Wallis test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KruskalWallis {
    /// Number of groups.
    pub groups: usize,
    /// Total number of observations.
    pub n: usize,
    /// H statistic, with the tie correction applied.
    pub h: f64,
    /// H statistic before the tie correction.
    pub h_uncorrected: f64,
    /// p-value from the chi-square distribution with `groups − 1` df.
    pub p_value: f64,
    /// Tie correction applied to the statistic.
    pub tie_correction: f64,
}

fn p_value_from_z(z: f64, alternative: Alternative) -> f64 {
    let cdf = normal_cdf(z);
    match alternative {
        Alternative::TwoSided => (2.0 * cdf.min(1.0 - cdf)).clamp(0.0, 1.0),
        Alternative::Less => cdf.clamp(0.0, 1.0),
        Alternative::Greater => (1.0 - cdf).clamp(0.0, 1.0),
    }
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

fn group_label(column: &Column, row: usize) -> Option<String> {
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

/// Mann–Whitney test of two independent samples (Wilcoxon rank-sum test).
///
/// The statistic is `U₁ = R₁ − n₁(n₁+1)/2`; the p-value uses the normal
/// approximation with a tie correction and a continuity correction. The
/// Hodges–Lehmann estimate of the shift is the median of the `n₁·n₂` pairwise
/// differences; it is skipped for very large samples.
pub fn mann_whitney(
    first: &Sample,
    second: &Sample,
    alternative: Alternative,
) -> Result<MannWhitney> {
    let n1 = first.n();
    let n2 = second.n();
    if n1 == 0 || n2 == 0 {
        return Err(Error::EmptySample);
    }
    let mut combined = Vec::with_capacity(n1 + n2);
    combined.extend_from_slice(first.values());
    combined.extend_from_slice(second.values());
    let ranks = mid_ranks(&combined);
    let rank_sum1: f64 = ranks[..n1].iter().sum();
    let u1 = rank_sum1 - n1 as f64 * (n1 as f64 + 1.0) / 2.0;
    let u2 = n1 as f64 * n2 as f64 - u1;

    let n = (n1 + n2) as f64;
    let ties = tie_term(&combined);
    let tie_correction = ties / (n * (n - 1.0));
    let variance = n1 as f64 * n2 as f64 / 12.0 * ((n + 1.0) - tie_correction);
    if variance <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let mean = n1 as f64 * n2 as f64 / 2.0;
    // Continuity correction: the correction always reduces the distance to
    // the mean, in the direction of the observed statistic.
    let corrected = if u1 > mean {
        u1 - mean - 0.5
    } else {
        u1 - mean + 0.5
    };
    let z = corrected / variance.sqrt();
    let p_value = p_value_from_z(z, alternative);

    let hodges_lehmann = if n1.saturating_mul(n2) <= 4_000_000 {
        let mut differences = Vec::with_capacity(n1 * n2);
        for &x in first.values() {
            for &y in second.values() {
                differences.push(x - y);
            }
        }
        differences.sort_by(|a, b| a.total_cmp(b));
        median_of_sorted(&differences)
    } else {
        f64::NAN
    };

    Ok(MannWhitney {
        n1,
        n2,
        u1,
        u2,
        rank_sum1,
        z,
        p_value,
        hodges_lehmann,
        tie_correction,
    })
}

/// Median of a sorted slice (average of the two central values when even).
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Wilcoxon signed-rank test of paired differences.
pub fn wilcoxon_signed_rank(
    differences: &[f64],
    alternative: Alternative,
) -> Result<WilcoxonSignedRank> {
    let nonzero: Vec<f64> = differences
        .iter()
        .copied()
        .filter(|value| value.abs() > 0.0)
        .collect();
    let n = nonzero.len();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let magnitudes: Vec<f64> = nonzero.iter().map(|value| value.abs()).collect();
    let ranks = mid_ranks(&magnitudes);
    let mut w_plus = 0.0;
    let mut w_minus = 0.0;
    for (value, rank) in nonzero.iter().zip(&ranks) {
        if *value > 0.0 {
            w_plus += rank;
        } else {
            w_minus += rank;
        }
    }
    let nf = n as f64;
    let ties = tie_term(&magnitudes);
    let tie_correction = ties / 48.0;
    let variance = nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0 - tie_correction;
    if variance <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let mean = nf * (nf + 1.0) / 4.0;
    let corrected = if w_plus > mean {
        w_plus - mean - 0.5
    } else {
        w_plus - mean + 0.5
    };
    let z = corrected / variance.sqrt();
    let p_value = p_value_from_z(z, alternative);

    let mut sorted = nonzero.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median_difference = median_of_sorted(&sorted);

    Ok(WilcoxonSignedRank {
        n,
        w_plus,
        w_minus,
        z,
        p_value,
        median_difference,
        tie_correction,
    })
}

/// Kruskal–Wallis test of `k` independent samples.
pub fn kruskal_wallis(groups: &BTreeMap<String, Vec<f64>>) -> Result<KruskalWallis> {
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    let mut combined = Vec::new();
    let mut rank_sums = Vec::with_capacity(groups.len());
    let mut sizes = Vec::with_capacity(groups.len());
    for values in groups.values() {
        if values.is_empty() {
            continue;
        }
        combined.extend_from_slice(values);
        sizes.push(values.len());
    }
    let n = combined.len();
    let k = sizes.len();
    if n <= k {
        return Err(Error::SampleTooSmall { n, min: k + 1 });
    }
    let ranks = mid_ranks(&combined);
    let mut offset = 0;
    for &size in &sizes {
        let sum: f64 = ranks[offset..offset + size].iter().sum();
        rank_sums.push(sum);
        offset += size;
    }
    let nf = n as f64;
    let mut sum = 0.0;
    for (ranks_sum, &size) in rank_sums.iter().zip(&sizes) {
        sum += ranks_sum * ranks_sum / size as f64;
    }
    let h_uncorrected = 12.0 / (nf * (nf + 1.0)) * sum - 3.0 * (nf + 1.0);
    let ties = tie_term(&combined);
    let tie_correction = 1.0 - ties / (nf * nf * nf - nf);
    let h = if tie_correction > 0.0 {
        h_uncorrected / tie_correction
    } else {
        h_uncorrected
    };
    // The Kruskal–Wallis statistic has an upper tail: the p-value is
    // P(χ² ≥ H) with `k − 1` degrees of freedom.
    let p_value = (1.0 - chi_square_cdf(h, (k - 1) as f64)).clamp(0.0, 1.0);
    Ok(KruskalWallis {
        groups: k,
        n,
        h,
        h_uncorrected,
        p_value,
        tie_correction,
    })
}

/// Outcome of the Friedman test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Friedman {
    /// Number of groups.
    pub groups: usize,
    /// Number of complete blocks.
    pub blocks: usize,
    /// Test statistic, with the tie correction applied.
    pub q: f64,
    /// Test statistic before the tie correction.
    pub q_uncorrected: f64,
    /// p-value from the chi-square distribution with `groups − 1` df.
    pub p_value: f64,
    /// Tie correction applied to the statistic.
    pub tie_correction: f64,
}

/// Friedman test of `k` groups measured on the same blocks.
///
/// Every row is a block; the values inside a block are ranked and the rank
/// sums per group are compared. `Q = 12/(n·k(k+1))·ΣRⱼ² − 3n(k+1)`, divided
/// by the tie correction `1 − Σ(t³−t)/(n(k³−k))`.
pub fn friedman(groups: &[Vec<f64>]) -> Result<Friedman> {
    let k = groups.len();
    if k < 2 {
        return Err(Error::Unsupported(
            "the Friedman test needs at least two groups".to_owned(),
        ));
    }
    let blocks = groups.iter().map(|values| values.len()).min().unwrap_or(0);
    if blocks < 2 {
        return Err(Error::SampleTooSmall { n: blocks, min: 2 });
    }
    let mut rank_sums = vec![0.0; k];
    let mut tie_total = 0.0;
    for block in 0..blocks {
        let row: Vec<f64> = groups.iter().map(|values| values[block]).collect();
        let ranks = mid_ranks(&row);
        tie_total += tie_term(&row);
        for (group, rank) in ranks.iter().enumerate() {
            rank_sums[group] += rank;
        }
    }
    let nf = blocks as f64;
    let kf = k as f64;
    let sum: f64 = rank_sums.iter().map(|ranks| ranks * ranks).sum();
    let q_uncorrected = 12.0 / (nf * kf * (kf + 1.0)) * sum - 3.0 * nf * (kf + 1.0);
    let tie_correction = 1.0 - tie_total / (nf * (kf * kf * kf - kf));
    let q = if tie_correction > 0.0 {
        q_uncorrected / tie_correction
    } else {
        q_uncorrected
    };
    let p_value = (1.0 - chi_square_cdf(q, (k - 1) as f64)).clamp(0.0, 1.0);
    Ok(Friedman {
        groups: k,
        blocks,
        q,
        q_uncorrected,
        p_value,
        tie_correction,
    })
}

/// Outcome of the one-sample sign test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignTest {
    /// Observations compared with the hypothesised median.
    pub n: usize,
    /// Observations above the hypothesised median.
    pub above: usize,
    /// Observations below the hypothesised median.
    pub below: usize,
    /// Observations equal to it (not used in the test).
    pub ties: usize,
    /// Hypothesised median.
    pub median: f64,
    /// Smaller of the two tails.
    pub statistic: usize,
    /// Two-sided exact p-value of the binomial with `p = 1/2`.
    pub p_value: f64,
}

/// One-sample sign test of a median.
///
/// Every observation is classified as *above* or *below* the hypothesised
/// median; under the null each sign is equally likely, so the number of
/// observations above the median follows a binomial law with `p = 1/2`.
/// Observations exactly on the median carry no sign and are excluded. The
/// p-value is exact (no normal approximation) and two-sided.
pub fn sign_test(sample: &Sample, median: f64) -> Result<SignTest> {
    if !median.is_finite() {
        return Err(Error::InvalidParameter {
            name: "hypothesised median",
            value: median,
        });
    }
    let mut above = 0usize;
    let mut below = 0usize;
    let mut ties = 0usize;
    for value in sample.values() {
        if *value > median {
            above += 1;
        } else if *value < median {
            below += 1;
        } else {
            ties += 1;
        }
    }
    let n = above + below;
    if n == 0 {
        return Err(Error::ConstantSample);
    }
    let statistic = above.min(below);
    let p_value = binomial_two_sided(statistic, n);
    Ok(SignTest {
        n,
        above,
        below,
        ties,
        median,
        statistic,
        p_value,
    })
}

/// Exact two-sided binomial p-value for at most `extreme` successes in `n`
/// trials with `p = 1/2`.
fn binomial_two_sided(extreme: usize, n: usize) -> f64 {
    // P(X ≤ extreme) + P(X ≥ n − extreme), with the tails disjoint.
    let mut lower = 0.0;
    let mut coefficient = 1.0f64;
    for k in 0..=n {
        if k > 0 {
            coefficient = coefficient * (n - k + 1) as f64 / k as f64;
        }
        let probability = coefficient * 0.5f64.powi(n as i32);
        if k <= extreme {
            lower += probability;
        }
    }
    let mut upper = 0.0;
    let mut coefficient = 1.0f64;
    for k in 0..=n {
        if k > 0 {
            coefficient = coefficient * (n - k + 1) as f64 / k as f64;
        }
        let probability = coefficient * 0.5f64.powi(n as i32);
        if k >= n - extreme {
            upper += probability;
        }
    }
    (lower + upper).clamp(0.0, 1.0)
}

/// Runs the sign test and builds the report.
pub fn sign_test_columns(df: &DataFrame, column_name: &str, median: f64) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let result = sign_test(&sample, median)?;
    let mut report = AnalysisResult::new(
        format!("Sign Test — column {column_name}"),
        format!("One-sample sign test of the median {median}"),
    );
    report.n = result.n;
    report.excluded = sample.excluded();
    report.statistics = vec![
        ("Hypothesised median", result.median),
        ("Above", result.above as f64),
        ("Below", result.below as f64),
        ("Ties (not used)", result.ties as f64),
        ("Statistic (smaller tail)", result.statistic as f64),
        ("p-value", result.p_value),
    ];
    let decision = if result.p_value < 0.05 {
        "rejects"
    } else {
        "does not reject"
    };
    report.extra.push(Item::Message(format!(
        "{} observations are above and {} below the hypothesised median; the test {} the \
         hypothesis at the 5% level.",
        result.above, result.below, decision
    )));
    report.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the median equals the hypothesised value; H₁: it does not".to_owned(),
        ),
        (
            "Statistic".to_owned(),
            "the number of signs above the median follows a binomial law with p = 1/2; the \
             p-value is exact and two-sided"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; values equal to the hypothesised median are \
             not signed"
                .to_owned(),
        ),
    ];
    report.warnings = standard_warnings(report.n, report.excluded, result.ties);
    Ok(report)
}

/// Outcome of Mood's median test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoodsMedian {
    /// Number of groups.
    pub groups: usize,
    /// Total number of observations.
    pub n: usize,
    /// Grand median used to split the observations.
    pub median: f64,
    /// Test statistic.
    pub statistic: f64,
    /// p-value from the chi-square distribution with `groups − 1` df.
    pub p_value: f64,
}

/// Mood's median test of `k` independent samples.
///
/// The observations are split at the grand median; the test compares the
/// counts below and above it across the groups.
pub fn moods_median(groups: &BTreeMap<String, Vec<f64>>) -> Result<MoodsMedian> {
    let k = groups.len();
    if k < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    let mut combined = Vec::new();
    for values in groups.values() {
        combined.extend_from_slice(values);
    }
    let n = combined.len();
    if n < k {
        return Err(Error::SampleTooSmall { n, min: k + 1 });
    }
    let mut sorted = combined.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    let mut below = Vec::with_capacity(k);
    let mut above = Vec::with_capacity(k);
    for values in groups.values() {
        let low = values.iter().filter(|value| **value < median).count() as f64;
        let high = values.iter().filter(|value| **value > median).count() as f64;
        below.push(low);
        above.push(high);
    }
    let mean_below = below.iter().sum::<f64>() / k as f64;
    let mean_above = above.iter().sum::<f64>() / k as f64;
    let mut statistic = 0.0;
    for &count in &below {
        if mean_below > 0.0 {
            statistic += (count - mean_below).powi(2) / mean_below;
        }
    }
    for &count in &above {
        if mean_above > 0.0 {
            statistic += (count - mean_above).powi(2) / mean_above;
        }
    }
    let p_value = (1.0 - chi_square_cdf(statistic, (k - 1) as f64)).clamp(0.0, 1.0);
    Ok(MoodsMedian {
        groups: k,
        n,
        median,
        statistic,
        p_value,
    })
}

/// Friedman test of the given columns, measured on the same rows.
pub fn friedman_columns(df: &DataFrame, columns: &[String]) -> Result<AnalysisResult> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the Friedman test needs at least two columns".to_owned(),
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
    let data: Vec<&Column> = columns
        .iter()
        .map(|name| df.column(name))
        .collect::<Result<Vec<_>>>()?;
    let rows = df.n_rows();
    let mut groups: Vec<Vec<f64>> = vec![Vec::new(); columns.len()];
    for row in 0..rows {
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for column in &data {
            match cell(column, row) {
                Some(value) => values.push(value),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if complete {
            for (group, value) in groups.iter_mut().zip(values) {
                group.push(value);
            }
        }
    }
    let excluded = rows - groups.first().map_or(0, Vec::len);
    let result = friedman(&groups)?;

    let mut analysis = AnalysisResult::new(
        format!("Friedman Test — {}", columns.join(" vs ")),
        "Rank test of k groups measured on the same blocks".to_owned(),
    );
    analysis.n = result.blocks;
    analysis.excluded = excluded;
    analysis.statistics = vec![
        ("Groups", result.groups as f64),
        ("Blocks", result.blocks as f64),
        ("Q", result.q),
        ("Q (not adjusted)", result.q_uncorrected),
        ("df", (result.groups - 1) as f64),
        ("p-value", result.p_value),
    ];
    analysis.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the k groups come from the same distribution; H₁: at least one differs".to_owned(),
        ),
        (
            "Method".to_owned(),
            "chi-square approximation of the Friedman statistic with the tie correction".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-finite value in any column are excluded".to_owned(),
        ),
    ];
    analysis.warnings = standard_warnings(analysis.n, analysis.excluded, 0);
    Ok(analysis)
}

/// Mood's median test of a response by a factor.
pub fn moods_median_columns(
    df: &DataFrame,
    response: &str,
    factor: &str,
) -> Result<AnalysisResult> {
    if response == factor {
        return Err(Error::Unsupported(
            "the response and factor must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let factor_column = df.column(factor)?;
    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            continue;
        };
        let Some(level) = group_label(factor_column, row) else {
            continue;
        };
        groups.entry(level).or_default().push(value);
    }
    let result = moods_median(&groups)?;

    let mut analysis = AnalysisResult::new(
        format!("Mood's Median Test — {response} by {factor}"),
        "Test of equal medians across the levels of a factor".to_owned(),
    );
    analysis.n = result.n;
    analysis.excluded = 0;
    analysis.statistics = vec![
        ("Groups", result.groups as f64),
        ("Grand median", result.median),
        ("Chi-square", result.statistic),
        ("df", (result.groups - 1) as f64),
        ("p-value", result.p_value),
    ];
    let mut rows = Vec::with_capacity(groups.len());
    for (level, values) in &groups {
        let low = values
            .iter()
            .filter(|value| **value < result.median)
            .count();
        let high = values
            .iter()
            .filter(|value| **value > result.median)
            .count();
        rows.push(vec![
            level.clone(),
            values.len().to_string(),
            low.to_string(),
            high.to_string(),
        ]);
    }
    analysis.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Below the median".to_owned(),
            "Above the median".to_owned(),
        ],
        rows,
    });
    analysis.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the groups have the same median; H₁: at least one differs".to_owned(),
        ),
        (
            "Method".to_owned(),
            "chi-square approximation on the counts below and above the grand median".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-finite response or factor are excluded".to_owned(),
        ),
    ];
    analysis.warnings = standard_warnings(analysis.n, analysis.excluded, 0);
    Ok(analysis)
}

/// Hodges–Lehmann interval for the shift between two samples.
///
/// The endpoints are the `k`-th smallest and `k`-th largest pairwise
/// differences, where `k` comes from the exact distribution of the
/// Mann–Whitney statistic; the achieved confidence is reported because it is
/// usually a little above the nominal level. Above 100 observations per sample
/// the exact distribution is skipped (`None`), since it would be both slow and
/// indistinguishable from the normal approximation.
fn mann_whitney_shift_ci(
    first: &[f64],
    second: &[f64],
    confidence: f64,
) -> Option<(f64, f64, f64)> {
    let (n1, n2) = (first.len(), second.len());
    if n1 < 2 || n2 < 2 || n1 > 100 || n2 > 100 {
        return None;
    }
    let mut differences: Vec<f64> = first
        .iter()
        .flat_map(|a| second.iter().map(move |b| a - b))
        .collect();
    differences.sort_by(|a, b| a.total_cmp(b));
    let ranks = n1 + n2;
    let width = n1 * ranks;
    let mut ways = vec![vec![0.0_f64; width + 1]; n1 + 1];
    ways[0][0] = 1.0;
    for rank in 1..=ranks {
        for chosen in (1..=n1.min(rank)).rev() {
            for total in (rank..=width).rev() {
                if ways[chosen - 1][total - rank] != 0.0 {
                    ways[chosen][total] += ways[chosen - 1][total - rank];
                }
            }
        }
    }
    let all: f64 = ways[n1].iter().sum();
    let tail = |u: usize| {
        let limit = u + n1 * (n1 + 1) / 2;
        ways[n1].iter().take(limit + 1).sum::<f64>() / all
    };
    let half_alpha = (1.0 - confidence) / 2.0;
    let mut k = 1usize;
    for candidate in 1..=n1 * n2 {
        if tail(candidate - 1) <= half_alpha {
            k = candidate;
        }
    }
    if k > differences.len() {
        return None;
    }
    let achieved = 1.0 - 2.0 * tail(k - 1);
    Some((
        differences[k - 1],
        differences[differences.len() - k],
        achieved,
    ))
}

/// Mann–Whitney test of two columns.
pub fn mann_whitney_columns(
    df: &DataFrame,
    first: &str,
    second: &str,
    alternative: Alternative,
) -> Result<AnalysisResult> {
    if first == second {
        return Err(Error::Unsupported(
            "the two samples must be different columns".to_owned(),
        ));
    }
    let sample1 = df.column(first)?.sample(MissingPolicy::Drop)?;
    let sample2 = df.column(second)?.sample(MissingPolicy::Drop)?;
    let result = mann_whitney(&sample1, &sample2, alternative)?;

    let mut analysis = AnalysisResult::new(
        format!("Mann-Whitney Test — {first} vs {second}"),
        format!("Rank-sum test; {} alternative", alternative.as_str()),
    );
    analysis.n = result.n1 + result.n2;
    analysis.excluded = sample1.excluded() + sample2.excluded();
    analysis.statistics = vec![
        ("n 1", result.n1 as f64),
        ("n 2", result.n2 as f64),
        ("Rank sum 1", result.rank_sum1),
        ("U 1", result.u1),
        ("U 2", result.u2),
        ("z", result.z),
        ("p-value", result.p_value),
    ];
    if result.hodges_lehmann.is_finite() {
        analysis
            .statistics
            .push(("Hodges-Lehmann shift", result.hodges_lehmann));
        // Confidence comes from the caller's 95% default in the dialog and CLI.
        if let Some((lower, upper, achieved)) =
            mann_whitney_shift_ci(sample1.values(), sample2.values(), 0.95)
        {
            analysis.statistics.push(("Shift CI lower", lower));
            analysis.statistics.push(("Shift CI upper", upper));
            analysis
                .statistics
                .push(("Achieved confidence", achieved * 100.0));
        }
    }
    analysis.extra.push(Item::Table {
        headers: vec![
            "Sample".to_owned(),
            "n".to_owned(),
            "Rank sum".to_owned(),
            "U".to_owned(),
        ],
        rows: vec![
            vec![
                first.to_owned(),
                result.n1.to_string(),
                format!("{:.3}", result.rank_sum1),
                format!("{:.3}", result.u1),
            ],
            vec![
                second.to_owned(),
                result.n2.to_string(),
                format!(
                    "{:.3}",
                    result.n1 as f64 * result.n2 as f64 - result.rank_sum1
                ),
                format!("{:.3}", result.u2),
            ],
        ],
    });
    analysis.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: the two samples come from the same distribution; H₁: the first sample is \
                 shifted {}",
                match alternative {
                    Alternative::TwoSided => "in either direction",
                    Alternative::Less => "downwards",
                    Alternative::Greater => "upwards",
                }
            ),
        ),
        (
            "Method".to_owned(),
            "normal approximation with a continuity correction; the variance carries the tie \
             correction"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "each column filtered independently; non-finite values excluded".to_owned(),
        ),
    ];
    analysis.warnings = standard_warnings(analysis.n, analysis.excluded, 0);
    Ok(analysis)
}

/// Wilcoxon signed-rank test of two paired columns.
pub fn wilcoxon_signed_rank_columns(
    df: &DataFrame,
    first: &str,
    second: &str,
    alternative: Alternative,
) -> Result<AnalysisResult> {
    if first == second {
        return Err(Error::Unsupported(
            "the two samples must be different columns".to_owned(),
        ));
    }
    let a = df.column(first)?;
    let b = df.column(second)?;
    let mut differences = Vec::new();
    for row in 0..df.n_rows() {
        if let (Some(x), Some(y)) = (cell(a, row), cell(b, row)) {
            differences.push(x - y);
        }
    }
    let rows = df.n_rows();
    let result = wilcoxon_signed_rank(&differences, alternative)?;

    let mut analysis = AnalysisResult::new(
        format!("Wilcoxon Signed-Rank Test — {first} vs {second}"),
        format!("Signed-rank test; {} alternative", alternative.as_str()),
    );
    analysis.n = result.n;
    analysis.excluded = rows.saturating_sub(differences.len());
    analysis.statistics = vec![
        ("n (non-zero differences)", result.n as f64),
        ("W+", result.w_plus),
        ("W−", result.w_minus),
        ("z", result.z),
        ("p-value", result.p_value),
        ("Median difference", result.median_difference),
    ];
    // The documented interval uses the Walsh averages: the endpoints are the
    // d-th and (M − d)-th of the n(n+1)/2 pairwise averages, with
    // d = ceil(M/2 − 0.5 − z·√(n(n+1)(2n+1)/24)) and the achieved confidence
    // from the normal approximation with a continuity correction.
    if differences.len() >= 2 {
        let mut nonzero: Vec<f64> = differences
            .iter()
            .copied()
            .filter(|value| *value != 0.0)
            .collect();
        nonzero.sort_by(|a, b| a.total_cmp(b));
        let n = nonzero.len();
        let mut walsh = Vec::with_capacity(n * (n + 1) / 2);
        for (index, value) in nonzero.iter().enumerate() {
            for other in &nonzero[index..] {
                walsh.push((value + other) / 2.0);
            }
        }
        walsh.sort_by(|a, b| a.total_cmp(b));
        let m = walsh.len();
        let nf = n as f64;
        let sd = (nf * (nf + 1.0) * (2.0 * nf + 1.0) / 24.0).sqrt();
        let z = 1.959_963_984_540_054;
        let d = (m as f64 / 2.0 - 0.5 - z * sd).ceil().max(0.0) as usize;
        if d < m {
            let lower = walsh[d];
            let upper = walsh[m - d - 1];
            let tail = crate::special::normal_cdf((d as f64 + 0.5 - m as f64 / 2.0) / sd);
            let achieved = (1.0 - 2.0 * tail).clamp(0.0, 1.0);
            analysis.statistics.push(("Median CI lower", lower));
            analysis.statistics.push(("Median CI upper", upper));
            analysis
                .statistics
                .push(("Achieved confidence", achieved * 100.0));
        }
    }
    analysis.notes = vec![
        (
            "Hypotheses".to_owned(),
            format!(
                "H₀: the median difference is zero; H₁: it is {}",
                match alternative {
                    Alternative::TwoSided => "different from zero",
                    Alternative::Less => "negative",
                    Alternative::Greater => "positive",
                }
            ),
        ),
        (
            "Method".to_owned(),
            "normal approximation with a continuity correction; the variance carries the tie \
             correction"
                .to_owned(),
        ),
        (
            "Pairs".to_owned(),
            format!(
                "{} complete pairs; zero differences are dropped before the ranking",
                differences.len()
            ),
        ),
    ];
    analysis.warnings = standard_warnings(analysis.n, analysis.excluded, 0);
    Ok(analysis)
}

/// Kruskal–Wallis test of a response by a factor.
pub fn kruskal_wallis_columns(
    df: &DataFrame,
    response: &str,
    factor: &str,
) -> Result<AnalysisResult> {
    if response == factor {
        return Err(Error::Unsupported(
            "the response and factor must be different columns".to_owned(),
        ));
    }
    let response_column = df.column(response)?;
    let factor_column = df.column(factor)?;
    let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(value) = cell(response_column, row) else {
            continue;
        };
        let Some(level) = group_label(factor_column, row) else {
            continue;
        };
        groups.entry(level).or_default().push(value);
    }
    if groups.len() < 2 {
        return Err(Error::Unsupported(
            "the factor must have at least two levels".to_owned(),
        ));
    }
    let result = kruskal_wallis(&groups)?;

    let mut analysis = AnalysisResult::new(
        format!("Kruskal-Wallis Test — {response} by {factor}"),
        "Rank test of k independent samples".to_owned(),
    );
    analysis.n = result.n;
    analysis.excluded = 0;
    analysis.statistics = vec![
        ("Groups", result.groups as f64),
        ("H", result.h),
        ("H (not adjusted)", result.h_uncorrected),
        ("df", (result.groups - 1) as f64),
        ("p-value", result.p_value),
    ];
    let mut rows = Vec::with_capacity(groups.len());
    for (level, values) in &groups {
        let mut sorted = values.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        rows.push(vec![
            level.clone(),
            values.len().to_string(),
            format!("{:.6}", median_of_sorted(&sorted)),
            format!("{:.6}", values.iter().sum::<f64>() / values.len() as f64),
        ]);
    }
    analysis.extra.push(Item::Table {
        headers: vec![
            "Level".to_owned(),
            "n".to_owned(),
            "Median".to_owned(),
            "Mean".to_owned(),
        ],
        rows,
    });
    analysis.notes = vec![
        (
            "Hypotheses".to_owned(),
            "H₀: the k groups come from the same distribution; H₁: at least one differs".to_owned(),
        ),
        (
            "Method".to_owned(),
            "chi-square approximation with the tie correction".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing or non-finite response or factor are excluded".to_owned(),
        ),
    ];
    analysis.warnings = standard_warnings(analysis.n, analysis.excluded, 0);
    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn sample(values: Vec<f64>) -> Sample {
        Sample::new(values, MissingPolicy::Drop).expect("sample")
    }

    #[test]
    fn the_sign_test_matches_the_binomial_tails() {
        // Six above and one below: the two-sided p-value is 2·P(X ≤ 1) with n = 7.
        let values = sample(vec![11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 9.0]);
        let result = sign_test(&values, 10.0).expect("test");
        assert_eq!(result.n, 7);
        assert_eq!(result.above, 6);
        assert_eq!(result.below, 1);
        assert_eq!(result.ties, 0);
        assert_eq!(result.statistic, 1);
        // P(X ≤ 1) = 8/128, and the other tail is the same.
        assert!(
            (result.p_value - 16.0 / 128.0).abs() < 1e-12,
            "{}",
            result.p_value
        );
        // A value on the median carries no sign.
        let values = sample(vec![10.0, 11.0, 12.0]);
        let result = sign_test(&values, 10.0).expect("test");
        assert_eq!(result.ties, 1);
        assert_eq!(result.n, 2);
        // Both remaining observations are above: 2·P(X ≤ 0) = 0.5.
        assert!((result.p_value - 0.5).abs() < 1e-12, "{}", result.p_value);
        // A constant sample has no signs at all.
        let values = sample(vec![5.0, 5.0]);
        assert!(matches!(
            sign_test(&values, 5.0),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn the_sign_test_report_is_built() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![8.0, 9.0, 11.0, 12.0, 13.0, 14.0, 15.0],
        )])
        .expect("frame");
        let report = sign_test_columns(&df, "x", 10.0).expect("report");
        let text = report.to_session_text();
        assert!(text.contains("Sign Test"), "{text}");
        assert!(text.contains("p-value"), "{text}");
    }

    #[test]
    fn mid_ranks_average_the_ties() {
        let ranks = mid_ranks(&[1.0, 2.0, 2.0, 5.0]);
        assert_eq!(ranks, vec![1.0, 2.5, 2.5, 4.0]);
        let single = mid_ranks(&[7.0]);
        assert_eq!(single, vec![1.0]);
    }

    #[test]
    fn mann_whitney_matches_the_documented_shift_interval() {
        // Documented: 12 vs 12, W 101.50, shift -1.175, CI (-1.95, -0.40),
        // achieved confidence 95.36 %.
        let first = [
            10.1, 10.3, 10.5, 10.7, 10.9, 11.1, 11.3, 11.5, 11.7, 11.9, 12.1, 12.3,
        ];
        let second = [
            11.0, 11.25, 11.5, 11.75, 12.0, 12.25, 12.5, 12.75, 13.0, 13.25, 13.5, 13.75,
        ];
        let df = DataFrame::new(vec![
            Column::new_f64("a", first.to_vec()),
            Column::new_f64("b", second.to_vec()),
        ])
        .expect("frame");
        let result = mann_whitney_columns(&df, "a", "b", Alternative::TwoSided).expect("test");
        let value = |name: &str| {
            result
                .statistics
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("{name} missing"))
        };
        assert!((value("Rank sum 1") - 101.5).abs() < 1e-9);
        assert!((value("Hodges-Lehmann shift") - (-1.175)).abs() < 1e-9);
        assert!((value("Shift CI lower") - (-1.95)).abs() < 1e-9);
        assert!((value("Shift CI upper") - (-0.40)).abs() < 1e-9);
        assert!((value("Achieved confidence") - 95.51).abs() < 0.05);
    }

    #[test]
    fn mann_whitney_detects_a_separated_sample() {
        let first = sample(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let second = sample(vec![11.0, 12.0, 13.0, 14.0, 15.0, 16.0]);
        let result = mann_whitney(&first, &second, Alternative::TwoSided).expect("test");
        assert_eq!(result.u1, 0.0);
        assert!(result.p_value < 0.01, "p {}", result.p_value);
        assert!(result.z < 0.0);
    }

    #[test]
    fn mann_whitney_accepts_identical_samples() {
        let first = sample(vec![1.0, 2.0, 3.0, 4.0]);
        let second = sample(vec![1.0, 2.0, 3.0, 4.0]);
        let result = mann_whitney(&first, &second, Alternative::TwoSided).expect("test");
        assert!((result.u1 - 8.0).abs() < 1e-12, "U1 {}", result.u1);
        assert!(result.p_value > 0.5, "p {}", result.p_value);
        assert!((result.hodges_lehmann - 0.0).abs() < 1e-12);
    }

    #[test]
    fn mann_whitney_hodges_lehmann_is_the_median_difference() {
        let first = sample(vec![1.0, 2.0, 3.0]);
        let second = sample(vec![2.0, 4.0, 6.0]);
        let result = mann_whitney(&first, &second, Alternative::TwoSided).expect("test");
        // Differences: -1, -3, -5, 0, -2, -4 → median -2.
        assert!(
            (result.hodges_lehmann + 2.0).abs() < 1e-12,
            "{}",
            result.hodges_lehmann
        );
    }

    #[test]
    fn wilcoxon_signed_rank_detects_a_consistent_shift() {
        let differences = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let result = wilcoxon_signed_rank(&differences, Alternative::TwoSided).expect("test");
        assert_eq!(result.w_plus, 21.0);
        assert_eq!(result.w_minus, 0.0);
        assert!(result.p_value < 0.05, "p {}", result.p_value);
        assert!((result.median_difference - 3.5).abs() < 1e-12);
    }

    #[test]
    fn wilcoxon_signed_rank_drops_zero_differences() {
        let differences = vec![0.0, 1.0, -1.0, 2.0, -2.0];
        let result = wilcoxon_signed_rank(&differences, Alternative::TwoSided).expect("test");
        assert_eq!(result.n, 4);
        // |1| and |2| are ties: the mid-ranks are 1.5 and 3.5.
        assert!((result.w_plus - 5.0).abs() < 1e-12, "W+ {}", result.w_plus);
        assert!(
            (result.w_minus - 5.0).abs() < 1e-12,
            "W− {}",
            result.w_minus
        );
    }

    #[test]
    fn kruskal_wallis_detects_different_groups() {
        let mut groups = BTreeMap::new();
        groups.insert("a".to_owned(), vec![1.0, 2.0, 3.0, 4.0]);
        groups.insert("b".to_owned(), vec![20.0, 21.0, 22.0, 23.0]);
        let result = kruskal_wallis(&groups).expect("test");
        assert_eq!(result.groups, 2);
        assert!(result.h > 3.0, "H {}", result.h);
        assert!(result.p_value < 0.05, "p {}", result.p_value);
    }

    #[test]
    fn kruskal_wallis_needs_two_groups() {
        let mut groups = BTreeMap::new();
        groups.insert("a".to_owned(), vec![1.0, 2.0, 3.0]);
        assert!(matches!(
            kruskal_wallis(&groups),
            Err(Error::Unsupported { .. })
        ));
    }

    #[test]
    fn friedman_detects_different_groups() {
        let first = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let second = vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let third = vec![20.0, 21.0, 22.0, 23.0, 24.0, 25.0];
        let result = friedman(&[first, second, third]).expect("friedman");
        assert_eq!(result.groups, 3);
        assert_eq!(result.blocks, 6);
        assert!(result.q > 3.0, "q {}", result.q);
        assert!(result.p_value < 0.05, "p {}", result.p_value);
    }

    #[test]
    fn friedman_of_identical_groups_is_not_significant() {
        let first = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let second = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = friedman(&[first.clone(), second.clone()]).expect("friedman");
        assert!(result.q < 1e-12, "q {}", result.q);
        assert!(result.p_value > 0.5, "p {}", result.p_value);
    }

    #[test]
    fn moods_median_detects_different_groups() {
        let mut groups = BTreeMap::new();
        groups.insert("a".to_owned(), vec![1.0, 2.0, 3.0, 4.0]);
        groups.insert("b".to_owned(), vec![20.0, 21.0, 22.0, 23.0]);
        let result = moods_median(&groups).expect("mood");
        assert_eq!(result.groups, 2);
        assert!(result.statistic > 3.0, "statistic {}", result.statistic);
        assert!(result.p_value < 0.05, "p {}", result.p_value);
    }

    #[test]
    fn column_level_tests_build_the_report() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![10.0, 11.0, 12.0, 13.0, 14.0]),
            Column::new_text(
                "g",
                vec![
                    "x".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                    "x".to_owned(),
                ],
            ),
        ])
        .expect("frame");

        let text = mann_whitney_columns(&df, "a", "b", Alternative::TwoSided)
            .expect("mwu")
            .to_session_text();
        assert!(text.contains("Mann-Whitney Test"), "{text}");
        assert!(text.contains("p-value"), "{text}");

        let text = wilcoxon_signed_rank_columns(&df, "a", "b", Alternative::TwoSided)
            .expect("wilcoxon")
            .to_session_text();
        assert!(text.contains("Wilcoxon Signed-Rank Test"), "{text}");

        let text = kruskal_wallis_columns(&df, "a", "g")
            .expect("kruskal")
            .to_session_text();
        assert!(text.contains("Kruskal-Wallis Test"), "{text}");
    }
}
