// SPDX-License-Identifier: MIT
//! Degenerate and extreme inputs.
//!
//! Every analysis must either produce a finite answer or refuse with a
//! readable reason; it must never panic or print a silent `inf`/`NaN`. The
//! cases are the boundary and invalid-input cases of the native suite.

use fp_core::data::{Column, DType, DataFrame};
use fp_core::io::{CsvOptions, decode_bytes, read_csv_str};
use fp_core::spc::ControlChartKind;
use fp_core::workflow::{AnalysisSpec, run_analysis};

fn frame(columns: Vec<Column>) -> DataFrame {
    DataFrame::new(columns).expect("frame")
}

fn numbers(values: Vec<f64>) -> DataFrame {
    frame(vec![Column::new_f64("x", values)])
}

fn text_error(df: &DataFrame, spec: &AnalysisSpec) -> String {
    match run_analysis(df, spec) {
        Ok(output) => panic!("expected an error, got {:?}", output.results),
        Err(error) => error.to_string(),
    }
}

fn statistics(df: &DataFrame, spec: &AnalysisSpec) -> Vec<(String, f64)> {
    let output = run_analysis(df, spec).unwrap_or_else(|error| panic!("{spec:?}: {error}"));
    output.results[0]
        .statistics
        .iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect()
}

fn value(stats: &[(String, f64)], name: &str) -> f64 {
    stats
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| *value)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn a_constant_column_is_refused_with_a_reason() {
    let constant = numbers(vec![5.0; 6]);
    for (spec, needle) in [
        (
            AnalysisSpec::capability("x", Some(4.0), Some(6.0)),
            "constant",
        ),
        (AnalysisSpec::one_sample_t("x", 5.0), "constant"),
        (AnalysisSpec::normality_test("x"), "constant"),
    ] {
        let message = text_error(&constant, &spec);
        assert!(
            message.to_lowercase().contains(needle),
            "{spec:?} said {message:?}"
        );
    }

    // Describing it is still meaningful: the mean exists, the spread is zero.
    let stats = statistics(&constant, &AnalysisSpec::descriptive_statistics("x"));
    assert_eq!(value(&stats, "Mean"), 5.0);
    assert_eq!(value(&stats, "Standard deviation (n−1)"), 0.0);
}

#[test]
fn tiny_samples_report_what_they_can() {
    let one = numbers(vec![7.0]);
    let stats = statistics(&one, &AnalysisSpec::descriptive_statistics("x"));
    assert_eq!(value(&stats, "Mean"), 7.0);
    assert!(
        !stats
            .iter()
            .any(|(name, _)| name.contains("Standard deviation")),
        "n = 1 has no n−1 standard deviation"
    );

    let two = frame(vec![
        Column::new_f64("a", vec![1.0, 3.0]),
        Column::new_f64("b", vec![2.0, 5.0]),
    ]);
    let stats = statistics(&two, &AnalysisSpec::two_sample_t("a", "b"));
    let p = value(&stats, "p-value");
    assert!(p.is_finite() && (0.0..=1.0).contains(&p), "p = {p}");

    let single = numbers(vec![1.0]);
    let stats = statistics(&single, &AnalysisSpec::one_proportion("x", 0.5));
    assert!(value(&stats, "p-value").is_finite());
}

#[test]
fn non_finite_values_are_excluded_and_counted() {
    let df = numbers(vec![1.0, f64::NAN, 3.0, f64::INFINITY, 5.0]);
    let output = run_analysis(&df, &AnalysisSpec::descriptive_statistics("x")).expect("describe");
    assert_eq!(output.results[0].n, 3);
    assert_eq!(output.results[0].excluded, 2);
    assert_eq!(
        value(
            &statistics(&df, &AnalysisSpec::descriptive_statistics("x")),
            "Mean"
        ),
        3.0
    );

    // A column of nothing but non-finite values has no sample left.
    let empty = numbers(vec![f64::NAN, f64::INFINITY]);
    let message = text_error(&empty, &AnalysisSpec::descriptive_statistics("x"));
    assert!(message.to_lowercase().contains("empty"), "{message:?}");
}

#[test]
fn malformed_files_are_refused_with_a_reason() {
    let empty = read_csv_str("", CsvOptions::default()).expect_err("empty file");
    assert!(empty.to_string().contains("no columns"), "{empty}");

    let header_only = read_csv_str("x\n", CsvOptions::default()).expect("header");
    assert_eq!(header_only.n_rows(), 0);
    let message = text_error(&header_only, &AnalysisSpec::descriptive_statistics("x"));
    assert!(message.contains("empty sample"), "{message}");

    // 1e400 is infinite: dropped and counted, never stored as a number.
    let huge = read_csv_str("x\n1e400\n-1e400\n2\n", CsvOptions::default()).expect("read");
    let output = run_analysis(&huge, &AnalysisSpec::descriptive_statistics("x")).expect("describe");
    assert_eq!(output.results[0].n, 1);
    assert_eq!(output.results[0].excluded, 2);

    // A NUL byte turns the column into text instead of corrupting the frame.
    let control = read_csv_str("x\n1\u{0}2\n2\n", CsvOptions::default()).expect("read");
    assert_eq!(control.column("x").expect("x").dtype(), DType::Text);
}

#[test]
fn a_constant_column_never_produces_nan() {
    use fp_core::attribute::AttributeChartKind;

    let df = frame(vec![
        Column::new_f64("x", vec![5.0; 8]),
        Column::new_f64("y", vec![2.0; 8]),
        Column::new_text("g", vec!["a".to_string(); 8]),
    ]);
    let specs = vec![
        AnalysisSpec::descriptive_statistics("x"),
        AnalysisSpec::histogram("x"),
        AnalysisSpec::boxplot("x"),
        AnalysisSpec::capability("x", Some(4.0), Some(6.0)),
        AnalysisSpec::one_sample_t("x", 5.0),
        AnalysisSpec::two_sample_t("x", "y"),
        AnalysisSpec::anova("x", "g"),
        AnalysisSpec::regression("x", vec!["y".to_owned()]),
        AnalysisSpec::correlation(vec!["x".to_owned(), "y".to_owned()]),
        AnalysisSpec::control_chart("x", ControlChartKind::IndividualsMovingRange, 1),
        AnalysisSpec::attribute_chart(AttributeChartKind::C, "x"),
        AnalysisSpec::gage_rr("x", "y", "g"),
        AnalysisSpec::kaplan_meier("x"),
        AnalysisSpec::normality_test("x"),
        AnalysisSpec::pareto("g"),
        AnalysisSpec::chi_square_independence("g", "g"),
        AnalysisSpec::factorial("x", vec!["y".to_owned()]),
    ];
    for spec in specs {
        match run_analysis(&df, &spec) {
            Ok(output) => {
                for result in &output.results {
                    for (name, value) in &result.statistics {
                        assert!(
                            !value.is_nan(),
                            "{spec:?} left {name} as NaN in {}",
                            result.title
                        );
                    }
                }
            }
            Err(error) => {
                // A refusal is fine as long as it explains itself.
                assert!(
                    !error.to_string().trim().is_empty(),
                    "{spec:?} failed without a message"
                );
            }
        }
    }
}

#[test]
fn tiny_samples_are_either_refused_or_finite() {
    let df = frame(vec![
        Column::new_f64("x", vec![5.0]),
        Column::new_f64("y", vec![6.0]),
        Column::new_text("g", vec!["a".to_string()]),
    ]);
    let specs = vec![
        AnalysisSpec::descriptive_statistics("x"),
        AnalysisSpec::capability("x", Some(4.0), Some(6.0)),
        AnalysisSpec::one_sample_t("x", 5.0),
        AnalysisSpec::two_sample_t("x", "y"),
        AnalysisSpec::anova("x", "g"),
        AnalysisSpec::regression("x", vec!["y".to_owned()]),
        AnalysisSpec::control_chart("x", ControlChartKind::IndividualsMovingRange, 1),
        AnalysisSpec::gage_rr("x", "y", "g"),
        AnalysisSpec::kaplan_meier("x"),
        AnalysisSpec::normality_test("x"),
    ];
    for spec in specs {
        match run_analysis(&df, &spec) {
            Ok(output) => {
                for result in &output.results {
                    for (name, value) in &result.statistics {
                        assert!(
                            value.is_finite(),
                            "{spec:?} printed {name} = {value} with one row"
                        );
                    }
                }
            }
            Err(error) => assert!(!error.to_string().trim().is_empty()),
        }
    }
}

#[test]
fn extreme_magnitudes_keep_finite_moments() {
    let df = numbers(vec![1e-300, 1e300, -1e300]);
    let stats = statistics(&df, &AnalysisSpec::descriptive_statistics("x"));
    assert_eq!(value(&stats, "Mean"), 0.0);
    let sd = value(&stats, "Standard deviation (n−1)");
    assert!(sd.is_finite(), "sd overflowed to {sd}");
    // Two deviations of 1e300 with n − 1 = 2: sd = 1e300.
    assert!((sd / 1e300 - 1.0).abs() < 1e-12, "sd = {sd}");
    let lower = value(&stats, "95% CI for the standard deviation (lower)");
    let upper = value(&stats, "95% CI for the standard deviation (upper)");
    assert!(lower.is_finite() && upper.is_finite(), "{lower}..{upper}");
    assert!(lower <= sd && sd <= upper, "{lower}..{upper} around {sd}");
    assert!(!stats.iter().any(|(_, value)| value.is_nan()), "{stats:?}");
}

#[test]
fn dates_are_read_as_dates_and_refused_as_numbers() {
    let df = read_csv_str(
        "d\n1900-01-01\n2000-02-29\n2100-01-01\n",
        CsvOptions::default(),
    )
    .expect("read");
    assert_eq!(df.column("d").expect("column").dtype(), DType::Date);
    let message = text_error(&df, &AnalysisSpec::descriptive_statistics("d"));
    assert!(message.contains("date"), "{message:?}");
}

#[test]
fn numeric_looking_text_stays_text() {
    let df = read_csv_str(
        "x\n\"1,5\"\n\"1e3\"\n\" 12 \"\nID-1\n",
        CsvOptions::default(),
    )
    .expect("read");
    assert_eq!(df.column("x").expect("column").dtype(), DType::Text);
    assert_eq!(df.cell_text(0, 0).as_deref(), Some("1,5"));
    // CSV import trims surrounding blanks on purpose, so " 12 " is stored as
    // "12" while still staying a text cell.
    assert_eq!(df.cell_text(0, 2).as_deref(), Some("12"));
    let message = text_error(&df, &AnalysisSpec::descriptive_statistics("x"));
    assert!(message.contains("text"), "{message:?}");
}

#[test]
fn decimal_comma_and_semicolon_are_detected() {
    let df = read_csv_str(
        "x;y\n1,5;2,5\n2,5;3,5\n3,5;4,5\n",
        CsvOptions {
            delimiter: b';',
            decimal_comma: true,
            has_header: true,
        },
    )
    .expect("read");
    let stats = statistics(&df, &AnalysisSpec::descriptive_statistics("x"));
    assert_eq!(value(&stats, "Mean"), 2.5);
    assert_eq!(value(&stats, "Maximum"), 3.5);
}

#[test]
fn a_ragged_csv_is_refused_with_the_position() {
    let error =
        read_csv_str("a,b\n1,2\n3\n4,5,6\n", CsvOptions::default()).expect_err("ragged rows");
    let message = error.to_string();
    assert!(message.contains("fields"), "{message:?}");
    assert!(message.contains("line 3"), "{message:?}");
}

#[test]
fn a_byte_order_mark_and_windows_line_endings_are_stripped() {
    let bytes = b"\xEF\xBB\xBFx,y\r\n1,2\r\n3,4\r\n";
    let (text, encoding) = decode_bytes(bytes);
    assert_eq!(encoding, "UTF-8");
    let df = read_csv_str(&text, CsvOptions::default()).expect("read");
    assert_eq!(df.column("x").expect("column").name(), "x");
    assert_eq!(df.n_rows(), 2);
}

#[test]
fn cp1252_accented_fields_survive() {
    // "São" and "Açor" as CP1252 bytes, which are not valid UTF-8.
    let text = [
        b"name,x\nS".as_slice(),
        &[0xE3],
        b"o,1\nA".as_slice(),
        &[0xE7],
        b"or,2\n".as_slice(),
    ]
    .concat();
    let (decoded, encoding) = decode_bytes(&text);
    assert_eq!(encoding, "CP1252");
    let df = read_csv_str(&decoded, CsvOptions::default()).expect("read");
    assert_eq!(df.cell_text(0, 0).as_deref(), Some("São"));
    assert_eq!(df.cell_text(0, 1).as_deref(), Some("Açor"));
}

#[test]
fn ties_keep_the_non_parametric_tests_finite() {
    let df = frame(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 3.0, 4.0]),
        Column::new_f64("b", vec![2.0, 2.0, 3.0, 4.0, 4.0]),
        Column::new_text(
            "g",
            vec!["x".into(), "x".into(), "y".into(), "y".into(), "y".into()],
        ),
    ]);
    for (spec, needles) in [
        (
            AnalysisSpec::mann_whitney("a", "b"),
            vec!["p-value", "Rank sum 1"],
        ),
        (
            AnalysisSpec::wilcoxon_signed_rank("a", "b"),
            vec!["p-value"],
        ),
        (AnalysisSpec::kruskal_wallis("a", "g"), vec!["p-value"]),
    ] {
        let stats = statistics(&df, &spec);
        for needle in needles {
            let (_, value) = stats
                .iter()
                .find(|(name, _)| name.contains(needle))
                .unwrap_or_else(|| panic!("{needle} missing in {spec:?}: {stats:?}"));
            assert!(value.is_finite(), "{needle} = {value} in {spec:?}");
        }
        let p = stats
            .iter()
            .find(|(name, _)| name == "p-value")
            .map(|(_, value)| *value);
        if let Some(p) = p {
            assert!((0.0..=1.0).contains(&p), "p = {p} in {spec:?}");
        }
    }
}

#[test]
fn tied_ranks_use_mid_ranks_in_the_correlations() {
    let df = frame(vec![
        Column::new_f64("a", vec![1.0, 2.0, 3.0, 3.0, 4.0]),
        Column::new_f64("b", vec![2.0, 2.0, 3.0, 4.0, 4.0]),
    ]);
    let output = run_analysis(
        &df,
        &AnalysisSpec::correlation(vec!["a".into(), "b".into()]),
    )
    .expect("correlation");
    let text = output.results[0].to_session_text();
    for (label, expected) in [
        ("0.877058", "Pearson"),
        ("0.892218", "Spearman"),
        ("0.824958", "Kendall"),
    ] {
        assert!(text.contains(label), "{expected} {label} missing in {text}");
    }
}

#[test]
fn the_crosstab_matches_the_hand_calculation() {
    let df = read_csv_str(
        "machine,defect\nA,scratch\nA,dent\nB,dent\nB,dent\n",
        CsvOptions::default(),
    )
    .expect("read");
    let stats = statistics(
        &df,
        &AnalysisSpec::chi_square_independence("machine", "defect"),
    );
    assert!((value(&stats, "Chi-square") - 4.0 / 3.0).abs() < 1e-9);
    assert_eq!(value(&stats, "df"), 1.0);
    // Fisher's exact test on the 2×2 table is two-sided 1.0 here.
    assert!((value(&stats, "Fisher exact p (two-sided)") - 1.0).abs() < 1e-9);
}
