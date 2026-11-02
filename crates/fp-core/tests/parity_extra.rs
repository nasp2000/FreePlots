//! Parity checks against the captured outputs that were still unreviewed:
//! exponential reliability, two-way ANOVA (main effects) and the two designs.

use std::path::PathBuf;

use fp_core::data::DataFrame;
use fp_core::doe;
use fp_core::io::read_csv_file;
use fp_core::rare;
use fp_core::report::Item;
use fp_core::taguchi;
use fp_core::workflow::{AnalysisSpec, run_analysis};

fn dataset(name: &str) -> DataFrame {
    dataset_at(&format!("validation/parity19/datasets/{name}"))
}

fn dataset_at(relative: &str) -> DataFrame {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    read_csv_file(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .0
}

fn close(actual: f64, expected: f64, what: &str) {
    let tolerance = 5e-4 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs captured {expected}"
    );
}

fn statistic(output: &fp_core::workflow::AnalysisOutput, name: &str) -> f64 {
    output.results[0]
        .statistics
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
        .unwrap_or_else(|| panic!("{name} missing"))
}

/// Row of the first table whose first cell matches, as text cells.
fn table_row(output: &fp_core::workflow::AnalysisOutput, first: &str) -> Vec<String> {
    for item in &output.results[0].extra {
        if let Item::Table { rows, .. } = item {
            for row in rows {
                if row.first().is_some_and(|cell| cell == first) {
                    return row.clone();
                }
            }
        }
    }
    panic!("row {first:?} missing")
}

fn tables(output: &fp_core::workflow::AnalysisOutput) -> Vec<Vec<Vec<String>>> {
    output.results[0]
        .extra
        .iter()
        .filter_map(|item| match item {
            Item::Table { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn exponential_reliability_matches_the_capture() {
    let df = dataset_at("validation/datasets/exponential_n20.csv");
    let output = run_analysis(&df, &AnalysisSpec::exponential_reliability("data"))
        .expect("exponential reliability");

    close(statistic(&output, "Mean/scale (θ)"), 12.2807, "mean");
    close(
        statistic(&output, "Standard error"),
        2.74606,
        "standard error",
    );
    close(
        statistic(&output, "Log-likelihood"),
        -70.161,
        "log-likelihood",
    );
    close(statistic(&output, "θ CI lower"), 7.92301, "CI lower");
    close(statistic(&output, "θ CI upper"), 19.0353, "CI upper");

    let mean = table_row(&output, "Mean(MTTF)");
    close(mean[1].parse().expect("estimate"), 12.2807, "mean estimate");
    close(mean[2].parse().expect("se"), 2.74606, "mean se");
    close(mean[3].parse().expect("lower"), 7.92301, "mean lower");
    close(mean[4].parse().expect("upper"), 19.0353, "mean upper");

    let median = table_row(&output, "Median");
    close(median[1].parse().expect("median"), 8.51236, "median");
    close(median[2].parse().expect("median se"), 1.90342, "median se");
    close(
        median[3].parse().expect("median lower"),
        5.49181,
        "median lower",
    );
    close(
        median[4].parse().expect("median upper"),
        13.1942,
        "median upper",
    );

    close(
        table_row(&output, "First quartile (Q1)")[1]
            .parse()
            .expect("q1"),
        3.53295,
        "Q1",
    );
    close(
        table_row(&output, "Third quartile (Q3)")[1]
            .parse()
            .expect("q3"),
        17.0247,
        "Q3",
    );
    close(
        table_row(&output, "Interquartile range (IQR)")[1]
            .parse()
            .expect("iqr"),
        13.4918,
        "IQR",
    );

    // The percentile table: the first row is 1 % and the last is 99 %.
    let percentile = tables(&output)
        .into_iter()
        .find(|rows| rows.first().is_some_and(|row| row[0] == "1"))
        .expect("percentile table");
    close(
        percentile[0][1].parse().expect("p1"),
        0.123426,
        "1 % percentile",
    );
    close(
        percentile[0][2].parse().expect("p1 se"),
        0.0275988,
        "1 % standard error",
    );
    close(
        percentile.last().expect("last")[1].parse().expect("p99"),
        56.5549,
        "99 % percentile",
    );
    assert_eq!(percentile.len(), 27, "1–10, the deciles and 91–99");
}

#[test]
fn the_two_way_anova_matches_the_captured_main_effects() {
    let df = dataset("twoway.csv");
    let output =
        run_analysis(&df, &AnalysisSpec::two_way_anova("y", "A", "B")).expect("two-way ANOVA");
    // The captured command fits main effects only: error with 20 df.
    assert_eq!(table_row(&output, "Error")[1], "20");
    close(
        table_row(&output, "Error")[3].parse().expect("ms"),
        0.015,
        "error MS",
    );
    close(
        table_row(&output, "A")[4].parse().expect("f"),
        1600.0,
        "F(A)",
    );
    close(
        table_row(&output, "B")[4].parse().expect("f"),
        844.444,
        "F(B)",
    );
    close(statistic(&output, "R²"), 0.993955, "R²");
    close(
        statistic(&output, "Pooled standard deviation"),
        0.122474,
        "pooled standard deviation",
    );
    assert!(
        output.results[0]
            .statistics
            .iter()
            .all(|(name, _)| !name.contains("interaction")),
        "the main-effects model must not print an interaction row"
    );

    // The interaction is an option; it takes its own degrees of freedom.
    let with = run_analysis(
        &df,
        &AnalysisSpec::two_way_anova("y", "A", "B").with_interaction(true),
    )
    .expect("with interaction");
    assert_eq!(table_row(&with, "Error")[1], "18");
    close(table_row(&with, "A")[4].parse().expect("f"), 1440.0, "F(A)");
    close(statistic(&with, "F (interaction)"), 0.0, "interaction F");
}

#[test]
fn the_plackett_burman_design_matches_the_captured_summary() {
    let df = doe::plackett_burman_design(12, false, 1).expect("design");
    // The captured design summary: 11 factors, 12 runs, one replicate.
    assert_eq!(df.n_rows(), 12, "12 runs");
    assert_eq!(df.n_columns(), 13, "StdOrder, RunOrder and 11 factors");
    let factors: Vec<&str> = df.columns()[2..]
        .iter()
        .map(fp_core::data::Column::name)
        .collect();
    assert_eq!(
        factors,
        vec!["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K"],
        "the captured design has eleven factors"
    );
    for column in &df.columns()[2..] {
        for row in 0..df.n_rows() {
            let value = column.cell_text(row);
            assert!(
                value == "1" || value == "-1",
                "factor levels are coded -1/+1, found {value}"
            );
        }
    }
}

#[test]
fn the_l18_array_matches_the_captured_summary() {
    let array = taguchi::array(18).expect("L18");
    assert_eq!(array.runs, 18);
    assert_eq!(array.columns.len(), 8);
    // L18 is 2¹ × 3⁷: the first factor has two levels, the rest three.
    let two_level = array.columns[0]
        .levels
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(two_level.len(), 2, "factor A has two levels");
    for column in &array.columns[1..] {
        let levels: std::collections::BTreeSet<_> = column.levels.iter().copied().collect();
        assert_eq!(levels.len(), 3, "{} has three levels", column.name);
    }
    let design = taguchi::design(18, 8, false, 1).expect("design");
    assert_eq!(design.n_rows(), 18);
    assert_eq!(design.n_columns(), 10, "StdOrder, RunOrder and 8 factors");
}

#[test]
fn wilcoxon_matches_the_captured_median_interval() {
    let df = dataset("wilcoxon.csv");
    let output =
        run_analysis(&df, &AnalysisSpec::wilcoxon_signed_rank("a", "b")).expect("wilcoxon");
    close(statistic(&output, "W+"), 0.0, "W+");
    close(statistic(&output, "Median difference"), -1.175, "median");
    close(statistic(&output, "Median CI lower"), -1.3, "CI lower");
    close(statistic(&output, "Median CI upper"), -1.05, "CI upper");
    close(statistic(&output, "p-value"), 0.002526, "p-value");
}

#[test]
fn the_rank_tests_match_the_captured_statistics() {
    let ranks = dataset("ranks.csv");
    let kruskal =
        run_analysis(&ranks, &AnalysisSpec::kruskal_wallis("y", "g")).expect("kruskal-wallis");
    close(statistic(&kruskal, "H"), 21.07, "H adjusted");
    close(
        statistic(&kruskal, "H (not adjusted)"),
        21.06,
        "H unadjusted",
    );

    let friedman_df = dataset("friedman_stacked.csv");
    // The engine takes the wide form; rebuild it from the stacked table.
    let mut a = Vec::new();
    let mut b = Vec::new();
    let mut c = Vec::new();
    for row in 0..friedman_df.n_rows() {
        let treatment = friedman_df.cell_text(1, row).unwrap_or_default();
        let value = friedman_df
            .cell_text(2, row)
            .and_then(|text| text.parse::<f64>().ok())
            .expect("value");
        match treatment.as_str() {
            "a" => a.push(value),
            "b" => b.push(value),
            _ => c.push(value),
        }
    }
    let wide = DataFrame::new(vec![
        fp_core::data::Column::new_f64("a", a),
        fp_core::data::Column::new_f64("b", b),
        fp_core::data::Column::new_f64("c", c),
    ])
    .expect("frame");
    let friedman = run_analysis(
        &wide,
        &AnalysisSpec::friedman(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]),
    )
    .expect("friedman");
    close(statistic(&friedman, "Q"), 15.44, "Q adjusted");
    close(
        statistic(&friedman, "Q (not adjusted)"),
        15.05,
        "Q unadjusted",
    );

    let mood = run_analysis(&ranks, &AnalysisSpec::moods_median("y", "g")).expect("mood");
    close(statistic(&mood, "Chi-square"), 20.00, "Mood chi-square");
    close(
        statistic(&mood, "Grand median"),
        13.575,
        "Mood grand median",
    );
}

#[test]
fn the_g_chart_matches_the_captured_limits() {
    let cases: [(&str, Vec<f64>, f64, f64, f64); 4] = [
        (
            "between",
            vec![11.0, 10.0, 22.0, 29.0, 5.0, 7.0, 3.0, 15.0, 8.0, 12.0],
            9.0 / 132.0,
            8.82,
            92.58,
        ),
        ("probe2", vec![2.0, 2.0], 1.0 / 6.0, 2.816, 35.259),
        (
            "probe5",
            vec![3.0, 1.0, 4.0, 1.0, 5.0],
            4.0 / 19.0,
            1.939,
            26.958,
        ),
        (
            "events",
            vec![
                0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 1.0, 0.0, 3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0,
                0.0, 1.0, 0.0, 1.0,
            ],
            19.0 / 32.0,
            0.0,
            6.439,
        ),
    ];
    for (name, counts, probability, center, ucl) in cases {
        let df = DataFrame::new(vec![fp_core::data::Column::new_f64("g", counts)]).expect("frame");
        let chart = rare::build(&df, "g", None).unwrap_or_else(|error| panic!("{name}: {error}"));
        close(chart.probability, probability, &format!("{name} p"));
        close(chart.limits.center, center, &format!("{name} CL"));
        close(chart.limits.ucl, ucl, &format!("{name} UCL"));
        assert_eq!(chart.limits.lcl, 0.0, "{name} LCL");
    }
}

#[test]
fn the_goodness_of_fit_matches_the_capture() {
    let df = dataset("gof_counts.csv");
    let output = run_analysis(
        &df,
        &AnalysisSpec::chi_square_goodness_of_fit("count", vec![20.0; 5]),
    )
    .expect("goodness of fit");
    close(statistic(&output, "Chi-square"), 2.9, "chi-square");
    close(statistic(&output, "df"), 4.0, "degrees of freedom");
    close(statistic(&output, "p-value"), 0.575, "p-value");
}
