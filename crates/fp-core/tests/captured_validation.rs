//! Regression tests against the reviewed Session fixtures in `validation/freplots`.
//!
//! These fixtures are FreePlots outputs. They freeze the current
//! numerical/report/plot behaviour so that later changes are explicit. The
//! public validation method is described in `validation/PROVENANCE.md`.
//!
//! Each validation has three independent tests:
//! 1. the full Session text matches the captured file;
//! 2. n, parameters and Anderson–Darling values match the captured numbers;
//! 3. the plot structure/axes/SVG match the expected analysis shape.

use std::path::PathBuf;

use fp_core::io;
use fp_core::plot::{ProbabilityScale, Scale, render_svg};
use fp_core::workflow::{AnalysisOutput, AnalysisSpec, DistributionKind, run_analysis};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn normalize_report(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|line| !line.trim_start().starts_with("Detected encoding"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The captured Session fixtures belong to the private validation pack; the
/// public build sources carry only the datasets. Without the pack these
/// comparisons have nothing to check and stay dormant, but a capture that is
/// expected locally and missing is still an error.
fn captured(name: &str) -> Option<String> {
    let dir = root().join("validation/freplots");
    let path = dir.join(format!("{name}.txt"));
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(normalize_report(&text)),
        Err(_) if !dir.exists() => None,
        Err(error) => panic!("cannot read captured {name} at {path:?}: {error}"),
    }
}

fn run(dataset: &str, kind: Option<DistributionKind>) -> AnalysisOutput {
    let file = if dataset == "normality_n20" {
        "normal_n20"
    } else {
        dataset
    };
    let path = root()
        .join("validation/datasets")
        .join(format!("{file}.csv"));
    let (df, _) = io::read_csv_file(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
    let spec = match kind {
        Some(kind) => AnalysisSpec::probability_plot("data").with_distribution(kind),
        None => AnalysisSpec::normality_test("data"),
    };
    run_analysis(&df, &spec).unwrap_or_else(|error| panic!("{dataset}: {error}"))
}

fn captured_value(text: &str, label: &str) -> f64 {
    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix(label) {
            return rest
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("cannot parse {label:?} in {line:?}"));
        }
    }
    panic!("label {label:?} not found in captured output");
}

fn close(actual: f64, expected: f64) {
    let tolerance = 5e-6 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual} vs captured {expected} (tolerance {tolerance})"
    );
}

fn assert_numbers(dataset: &str, kind: Option<DistributionKind>) {
    let Some(text) = captured(dataset) else {
        return;
    };
    let out = run(dataset, kind);
    let result = &out.results[0];
    let n = captured_value(&text, "Observations (n)") as usize;
    assert_eq!(result.n, n);
    for (label, value) in &result.parameters {
        close(*value, captured_value(&text, label));
    }
    let ad = captured_value(&text, "Anderson–Darling (A²)");
    let result_ad: f64 = result.gof[0].1.parse().expect("AD is numeric");
    close(result_ad, ad);
    let p_line = text
        .lines()
        .find(|line| line.contains("p-value") && line.split_whitespace().last().is_some())
        .expect("captured p-value line");
    let captured_p: f64 = p_line
        .split_whitespace()
        .last()
        .expect("p-value")
        .parse()
        .expect("numeric p-value");
    let result_p: f64 = result.gof[1].1.parse().expect("p is numeric");
    close(result_p, captured_p);
}

macro_rules! probability_validation {
    ($module:ident, $dataset:literal, $kind:expr, $bands:expr, $log_x:expr, $gumbel:expr) => {
        mod $module {
            use super::*;

            #[test]
            fn report_matches_captured_output() {
                let Some(expected) = captured($dataset) else {
                    return;
                };
                let out = run($dataset, Some($kind));
                assert_eq!(
                    normalize_report(&out.results[0].to_session_text()).trim_end(),
                    expected.trim_end()
                );
            }

            #[test]
            fn numbers_match_captured_output() {
                assert_numbers($dataset, Some($kind));
            }

            #[test]
            fn plot_structure_matches_the_analysis() {
                let out = run($dataset, Some($kind));
                let plot = out.plot.expect("probability plot");
                assert_eq!(plot.points.len(), out.results[0].n);
                assert_eq!(!plot.bands.is_empty(), $bands);
                assert_eq!(matches!(plot.x_axis.scale, Scale::Log10), $log_x);
                if $gumbel {
                    assert!(matches!(
                        plot.y_axis.scale,
                        Scale::Probability(ProbabilityScale::Gumbel)
                    ));
                } else {
                    assert!(matches!(
                        plot.y_axis.scale,
                        Scale::Probability(ProbabilityScale::Normal)
                    ));
                }
                let svg = render_svg(&plot);
                assert!(svg.starts_with("<svg"));
                assert_eq!(svg.matches("<circle").count(), out.results[0].n);
            }
        }
    };
}

probability_validation!(
    normal_n8,
    "normal_n8",
    DistributionKind::Normal,
    true,
    false,
    false
);
probability_validation!(
    normal_n20,
    "normal_n20",
    DistributionKind::Normal,
    true,
    false,
    false
);
probability_validation!(
    normal_n30,
    "normal_n30",
    DistributionKind::Normal,
    true,
    false,
    false
);
probability_validation!(
    normal_n200,
    "normal_n200",
    DistributionKind::Normal,
    true,
    false,
    false
);
probability_validation!(
    lognormal_n30,
    "lognormal_n30",
    DistributionKind::Lognormal,
    true,
    true,
    false
);
probability_validation!(
    weibull_n20,
    "weibull_n20",
    DistributionKind::Weibull,
    true,
    true,
    true
);
probability_validation!(
    exponential_n20,
    "exponential_n20",
    DistributionKind::Exponential,
    false,
    true,
    true
);

mod normality_n20 {
    use super::*;

    #[test]
    fn report_matches_captured_output() {
        let Some(expected) = captured("normality_n20") else {
            return;
        };
        let out = run("normality_n20", None);
        assert_eq!(
            normalize_report(&out.results[0].to_session_text()).trim_end(),
            expected.trim_end()
        );
    }

    #[test]
    fn numbers_match_captured_output() {
        assert_numbers("normality_n20", None);
    }

    #[test]
    fn normality_has_no_plot_and_reports_percentile_lines() {
        let out = run("normality_n20", None);
        assert!(out.plot.is_none());
        let text = out.results[0].to_session_text();
        assert!(text.contains("Normality Test: Anderson–Darling"));
        assert!(text.contains("Percentile lines"));
    }
}

/// The captured Weibull plot (n = 20) measures the 95 % band at five
/// probabilities. The band is symmetric in `log10 x` (the delta method on the
/// axis coordinate); the half-widths are read from the captured PNG.
mod weibull_band_n20 {
    use super::*;
    use fp_core::plot::Point;

    fn interpolate(points: &[Point], y: f64) -> f64 {
        for window in points.windows(2) {
            let (p0, p1) = (window[0], window[1]);
            if (p0.y - y).abs() < 1e-12 {
                return p0.x;
            }
            if (p0.y - y) * (p1.y - y) <= 0.0 && (p1.y - p0.y).abs() > 0.0 {
                let t = (y - p0.y) / (p1.y - p0.y);
                return p0.x + t * (p1.x - p0.x);
            }
        }
        f64::NAN
    }

    fn half_width_at(plot: &fp_core::plot::PlotSpec, probability: f64) -> f64 {
        let band = &plot.bands[0];
        let y = (-(1.0 - probability).ln()).ln();
        (interpolate(&band.upper, y) - interpolate(&band.lower, y)) / 2.0
    }

    #[test]
    fn band_half_widths_match_the_captured_plot() {
        let out = run("weibull_n20", Some(DistributionKind::Weibull));
        let plot = out.plot.expect("probability plot");
        // Measured on the captured PNG, in log10 units of the data axis.
        let captured = [
            (0.2, 0.1077),
            (0.3, 0.0893),
            (0.5, 0.0680),
            (0.7, 0.0584),
            (0.8, 0.0578),
        ];
        for (probability, expected) in captured {
            let got = half_width_at(&plot, probability);
            assert!(
                (got - expected).abs() <= 0.002,
                "p={probability}: band half-width {got:.4}, captured {expected} (tolerance 0.002)"
            );
        }
    }

    #[test]
    fn band_is_symmetric_in_the_axis_coordinate() {
        let out = run("weibull_n20", Some(DistributionKind::Weibull));
        let plot = out.plot.expect("probability plot");
        let band = &plot.bands[0];
        for (lower, upper) in band.lower.iter().zip(&band.upper) {
            let center = 0.5 * (lower.x + upper.x);
            assert!((center - lower.x - (upper.x - center)).abs() < 1e-12);
            assert!(lower.x < upper.x);
        }
    }
}

/// Exponential capability (n = 20, LSL 0.5, USL 20), captured from the
/// captured run's "Process Capability Report" graph.
mod exponential_capability_n20 {
    use super::*;
    use fp_core::workflow::AnalysisSpec;

    fn capability() -> AnalysisOutput {
        let path = root().join("validation/datasets/exponential_n20.csv");
        let (df, _) = io::read_csv_file(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
        let spec = AnalysisSpec::capability("data", Some(0.5), Some(20.0))
            .with_capability_distribution(DistributionKind::Exponential)
            .with_capability_bootstrap(0);
        run_analysis(&df, &spec).unwrap_or_else(|error| panic!("capability: {error}"))
    }

    fn row(text: &str, label: &[&str]) -> Vec<f64> {
        for line in text.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() > label.len() && tokens[..label.len()] == *label {
                return tokens[label.len()..]
                    .iter()
                    .filter_map(|token| token.parse().ok())
                    .collect();
            }
        }
        panic!("no row {label:?} in:\n{text}");
    }

    #[test]
    fn numbers_match_the_captured_report() {
        let out = capability();
        let text = out.results[0].to_session_text();
        let mean = row(&text, &["Mean", "(θ)"])[0];
        assert!((mean - 12.2807).abs() <= 5e-5, "mean {mean}");
        // Expected and observed PPM: the captured report prints two decimals.
        for (label, expected, observed) in [
            (&["PPM", "<", "LSL"][..], 39896.45, 50000.0),
            (&["PPM", ">", "USL"][..], 196210.82, 250000.0),
            (&["PPM", "total"][..], 236107.28, 300000.0),
        ] {
            let values = row(&text, label);
            assert!(
                (values[0] - expected).abs() <= 0.6,
                "{label:?} expected {} got {}",
                expected,
                values[0]
            );
            assert!(
                (values[1] - observed).abs() <= 0.6,
                "{label:?} observed {} got {}",
                observed,
                values[1]
            );
        }
        for (index, expected) in [("Pp", 0.43), ("PPL", 0.58), ("PPU", 0.29), ("Ppk", 0.29)] {
            let value = row(&text, &[index])[0];
            assert!(
                (value - expected).abs() <= 0.006,
                "{index} captured {expected} got {value}"
            );
        }
    }

    #[test]
    fn report_states_the_exponential_model() {
        let out = capability();
        let text = out.results[0].to_session_text();
        assert!(text.contains("Capability Analysis: Exponential"), "{text}");
        assert!(text.contains("validated for Weibull, lognormal and exponential"));
    }
}

/// Control-chart limits read from the captured graphs (normal_n20: n = 20,
/// four subgroups of five for the X̄ charts).
mod control_charts_n20 {
    use super::*;
    use fp_core::spc::ControlChartKind;
    use fp_core::workflow::AnalysisSpec;

    fn report(kind: ControlChartKind, subgroup: usize) -> String {
        let path = root().join("validation/datasets/normal_n20.csv");
        let (df, _) = io::read_csv_file(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
        let spec = AnalysisSpec::control_chart("data", kind, subgroup);
        let out = run_analysis(&df, &spec).unwrap_or_else(|error| panic!("{kind:?}: {error}"));
        out.results[0].to_session_text()
    }

    fn value(text: &str, label: &[&str]) -> f64 {
        for line in text.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() > label.len() && tokens[..label.len()] == *label {
                return tokens[label.len()]
                    .parse()
                    .unwrap_or_else(|_| panic!("not numeric: {line}"));
            }
        }
        panic!("no row {label:?} in:\n{text}");
    }

    fn assert_close(actual: f64, expected: f64, what: &str) {
        assert!(
            (actual - expected).abs() <= 0.006,
            "{what}: {actual:.6} vs captured {expected}"
        );
    }

    #[test]
    fn i_mr_limits_match_the_captured_chart() {
        let text = report(ControlChartKind::IndividualsMovingRange, 1);
        assert_close(value(&text, &["Center", "line"]), 20.75, "X̄");
        assert_close(value(&text, &["Location", "LCL"]), 13.65, "LCL");
        assert_close(value(&text, &["Location", "UCL"]), 27.85, "UCL");
        assert_close(value(&text, &["Dispersion", "center", "line"]), 2.669, "MR̄");
        assert_close(value(&text, &["Dispersion", "LCL"]), 0.0, "MR LCL");
        assert_close(value(&text, &["Dispersion", "UCL"]), 8.722, "MR UCL");
        assert!(text.contains("No out-of-control points"), "{text}");
    }

    #[test]
    fn xbar_r_limits_match_the_captured_chart() {
        let text = report(ControlChartKind::XbarR, 5);
        assert_close(value(&text, &["Center", "line"]), 20.746, "X̄");
        assert_close(value(&text, &["Location", "LCL"]), 17.419, "LCL");
        assert_close(value(&text, &["Location", "UCL"]), 24.073, "UCL");
        assert_close(value(&text, &["Dispersion", "center", "line"]), 5.77, "R̄");
        assert_close(value(&text, &["Dispersion", "LCL"]), 0.0, "R LCL");
        assert_close(value(&text, &["Dispersion", "UCL"]), 12.20, "R UCL");
    }

    #[test]
    fn xbar_s_limits_match_the_captured_chart() {
        let text = report(ControlChartKind::XbarS, 5);
        assert_close(value(&text, &["Center", "line"]), 20.746, "X̄");
        assert_close(value(&text, &["Location", "LCL"]), 17.507, "LCL");
        assert_close(value(&text, &["Location", "UCL"]), 23.985, "UCL");
        assert_close(value(&text, &["Dispersion", "center", "line"]), 2.269, "S̄");
        assert_close(value(&text, &["Dispersion", "LCL"]), 0.0, "S LCL");
        assert_close(value(&text, &["Dispersion", "UCL"]), 4.741, "S UCL");
    }
}

/// Text analyses captured from the reference Session on the new fixtures.
mod text_analyses {
    use super::*;
    use fp_core::workflow::AnalysisSpec;

    fn dataset(name: &str) -> fp_core::data::DataFrame {
        let path = root()
            .join("validation/datasets")
            .join(format!("{name}.csv"));
        io::read_csv_file(&path)
            .unwrap_or_else(|error| panic!("{path:?}: {error}"))
            .0
    }

    fn report(df: &fp_core::data::DataFrame, spec: AnalysisSpec) -> String {
        run_analysis(df, &spec)
            .unwrap_or_else(|error| panic!("{spec:?}: {error}"))
            .results[0]
            .to_session_text()
    }

    fn number(text: &str, label: &[&str]) -> f64 {
        for line in text.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() > label.len() && tokens[..label.len()] == *label {
                return tokens[label.len()]
                    .parse()
                    .unwrap_or_else(|_| panic!("not numeric: {line}"));
            }
        }
        panic!("no row {label:?} in:\n{text}");
    }

    fn close(actual: f64, expected: f64, what: &str, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{what}: {actual:.6} vs captured {expected}"
        );
    }

    #[test]
    fn two_sample_t_matches_the_captured_report() {
        let text = report(&dataset("twosample"), AnalysisSpec::two_sample_t("a", "b"));
        close(number(&text, &["Mean", "1"]), 10.247, "mean a", 0.0005);
        close(
            number(&text, &["Standard", "deviation", "1"]),
            0.543,
            "sd a",
            0.0005,
        );
        close(number(&text, &["Mean", "2"]), 10.63, "mean b", 0.005);
        close(
            number(&text, &["Standard", "deviation", "2"]),
            1.23,
            "sd b",
            0.005,
        );
        close(
            number(&text, &["Difference", "(1", "−", "2)"]),
            -0.381,
            "difference",
            0.0005,
        );
        close(number(&text, &["t"]), -0.98, "t", 0.005);
        close(number(&text, &["df"]), 15.0, "df", 0.15);
        close(number(&text, &["p-value"]), 0.341, "p", 0.0005);
        close(number(&text, &["CI", "lower"]), -1.206, "CI low", 0.002);
        close(number(&text, &["CI", "upper"]), 0.445, "CI high", 0.002);
    }

    #[test]
    fn tukey_and_fisher_match_the_captured_tables() {
        let capture = match std::fs::read_to_string(
            root().join("validation/captured-output/parity19/tukey_fisher.txt"),
        ) {
            Ok(text) => text,
            Err(_) => return,
        };
        assert!(capture.contains("Tukey Pairwise Comparisons"));
        assert!(capture.contains("Fisher Pairwise Comparisons"));
        let df = dataset("oneway");
        let model = fp_core::anova::fit(&df, "response", "factor", 0.95).expect("one-way ANOVA");
        let pair = |a: &str, b: &str| {
            model
                .pairwise
                .iter()
                .find(|pair| {
                    (pair.first == a && pair.second == b) || (pair.first == b && pair.second == a)
                })
                .unwrap_or_else(|| panic!("missing pair {a}-{b}"))
        };
        let check = |a: &str,
                     b: &str,
                     difference: f64,
                     tukey: (f64, f64),
                     p_tukey: f64,
                     lsd: (f64, f64),
                     p_lsd: f64| {
            let pair = pair(a, b);
            assert!(
                (pair.difference.abs() - difference).abs() < 0.002,
                "{a}-{b} difference {} vs {difference}",
                pair.difference
            );
            let mut bounds = [pair.tukey_lower.abs(), pair.tukey_upper.abs()];
            bounds.sort_by(f64::total_cmp);
            let mut expected = [tukey.0, tukey.1];
            expected.sort_by(f64::total_cmp);
            assert!(
                (bounds[0] - expected[0]).abs() < 0.002 && (bounds[1] - expected[1]).abs() < 0.002,
                "{a}-{b} Tukey CI {bounds:?} vs {expected:?}"
            );
            assert!(
                (pair.p_tukey - p_tukey).abs() < 0.001,
                "{a}-{b} Tukey p {} vs {p_tukey}",
                pair.p_tukey
            );
            let mut bounds = [pair.lsd_lower.abs(), pair.lsd_upper.abs()];
            bounds.sort_by(f64::total_cmp);
            let mut expected = [lsd.0, lsd.1];
            expected.sort_by(f64::total_cmp);
            assert!(
                (bounds[0] - expected[0]).abs() < 0.002 && (bounds[1] - expected[1]).abs() < 0.002,
                "{a}-{b} LSD CI {bounds:?} vs {expected:?}"
            );
            assert!(
                (pair.p_raw - p_lsd).abs() < 0.001,
                "{a}-{b} LSD p {} vs {p_lsd}",
                pair.p_raw
            );
        };
        check(
            "A",
            "B",
            0.747,
            (0.172, 1.321),
            0.010,
            (0.272, 1.222),
            0.004,
        );
        check(
            "A",
            "C",
            1.549,
            (0.974, 2.124),
            0.000,
            (1.074, 2.024),
            0.000,
        );
        check(
            "B",
            "C",
            0.802,
            (0.228, 1.377),
            0.006,
            (0.328, 1.277),
            0.002,
        );
    }

    #[test]
    fn one_way_anova_matches_the_captured_report() {
        let text = report(
            &dataset("oneway"),
            AnalysisSpec::anova("response", "factor"),
        );
        close(
            number(&text, &["Pooled", "standard", "deviation"]),
            0.456644,
            "pooled sd",
            5e-6,
        );
        close(number(&text, &["F"]), 23.03, "F", 0.005);
        for (level, mean, sd, low, high) in [
            ("A", 19.949, 0.489, 19.613, 20.285),
            ("B", 20.696, 0.493, 20.360, 21.032),
            ("C", 21.498, 0.378, 21.162, 21.834),
        ] {
            let row = text
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{level} ")))
                .unwrap_or_else(|| panic!("level {level} missing:\n{text}"));
            let tokens: Vec<&str> = row.split_whitespace().collect();
            close(
                tokens[2].parse().unwrap(),
                mean,
                &format!("{level} mean"),
                0.0005,
            );
            close(
                tokens[3].parse().unwrap(),
                sd,
                &format!("{level} sd"),
                0.0005,
            );
            let bounds: Vec<f64> = row
                .split(['(', ',', ')'])
                .filter_map(|token| token.trim().parse().ok())
                .collect();
            close(bounds[0], low, &format!("{level} CI low"), 0.0005);
            close(bounds[1], high, &format!("{level} CI high"), 0.0005);
        }
    }

    #[test]
    fn regression_matches_the_captured_report() {
        let text = report(
            &dataset("regress"),
            AnalysisSpec::regression("y", vec!["x".to_owned()]),
        );
        close(
            number(&text, &["Predicted", "R²"]),
            0.9738,
            "predicted R²",
            5e-4,
        );
        close(
            number(&text, &["Residual", "standard", "error"]),
            0.491386,
            "S",
            5e-6,
        );
        let constant = text
            .lines()
            .find(|line| line.trim_start().starts_with("Constant "))
            .expect("constant row");
        let tokens: Vec<&str> = constant.split_whitespace().collect();
        close(tokens[1].parse().unwrap(), 3.301, "constant", 0.0005);
        close(tokens[2].parse().unwrap(), 0.281, "constant SE", 0.0005);
        close(tokens[3].parse().unwrap(), 11.73, "constant t", 0.005);
        assert!(text.contains("R large residual"), "{text}");
        let unusual = text
            .lines()
            .find(|line| line.trim_start().starts_with("15 "))
            .expect("observation 15");
        assert!(unusual.contains('R'), "{unusual}");
    }

    #[test]
    fn correlation_matches_the_captured_matrix() {
        let text = report(
            &dataset("correlation"),
            AnalysisSpec::correlation(vec!["x".to_owned(), "y".to_owned(), "z".to_owned()]),
        );
        for (first, second, expected) in [("x", "y", 0.990), ("x", "z", 0.965), ("y", "z", 0.960)] {
            let row = text
                .lines()
                .find(|line| {
                    line.trim_start()
                        .starts_with(&format!("{first} × {second}"))
                })
                .unwrap_or_else(|| panic!("pair {first}×{second} missing:\n{text}"));
            let tokens: Vec<&str> = row.split_whitespace().collect();
            close(
                tokens[4].parse().unwrap(),
                expected,
                &format!("{first}×{second}"),
                0.0005,
            );
        }
    }
}

/// Special-cause tests compared with the captured "Test Results" tables.
mod control_tests {
    use super::*;
    use fp_core::spc::{ControlChartKind, TestSet};
    use fp_core::workflow::AnalysisSpec;

    fn signals(
        dataset: &str,
        kind: ControlChartKind,
        subgroup: usize,
        tests: TestSet,
    ) -> Vec<(u8, String, usize)> {
        let path = root()
            .join("validation/datasets")
            .join(format!("{dataset}.csv"));
        let (df, _) = io::read_csv_file(&path).unwrap_or_else(|error| panic!("{path:?}: {error}"));
        let spec =
            AnalysisSpec::control_chart("data", kind, subgroup).with_control_chart_tests(tests);
        let out = run_analysis(&df, &spec).unwrap_or_else(|error| panic!("{dataset}: {error}"));
        let text = out.results[0].to_session_text();
        text.lines()
            .filter_map(|line| {
                let tokens: Vec<&str> = line.split_whitespace().collect();
                if tokens.len() >= 3 && (tokens[1] == "Location" || tokens[1] == "Dispersion") {
                    Some((
                        tokens[0].parse().ok()?,
                        tokens[1].to_owned(),
                        tokens[2].parse().ok()?,
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    fn points(signals: &[(u8, String, usize)], test: u8, chart: &str) -> Vec<usize> {
        signals
            .iter()
            .filter(|(number, name, _)| *number == test && name == chart)
            .map(|(_, _, point)| *point)
            .collect()
    }

    #[test]
    fn the_shift_sample_matches_the_captured_tests() {
        let signals = signals(
            "control_shift",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::all(),
        );
        assert_eq!(points(&signals, 1, "Location"), [10, 13, 22, 30]);
        assert_eq!(
            points(&signals, 2, "Location"),
            [9, 10, 11, 12, 13, 14, 15, 24, 25, 26, 27, 28, 29]
        );
        assert_eq!(
            points(&signals, 5, "Location"),
            [12, 13, 19, 23, 24, 25, 27]
        );
        assert_eq!(
            points(&signals, 6, "Location"),
            [
                6, 7, 8, 10, 11, 12, 13, 14, 15, 19, 20, 21, 22, 23, 24, 25, 27, 28, 29
            ]
        );
        assert_eq!(
            points(&signals, 8, "Location"),
            [17, 18, 19, 20, 21, 22, 23, 24, 25]
        );
        assert!(points(&signals, 3, "Location").is_empty());
        assert!(points(&signals, 4, "Location").is_empty());
        assert!(points(&signals, 7, "Location").is_empty());
        assert_eq!(points(&signals, 1, "Dispersion"), [30]);
    }

    #[test]
    fn the_default_test_set_is_test_one_only() {
        let signals = signals(
            "control_shift",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::default(),
        );
        assert_eq!(points(&signals, 1, "Location"), [10, 13, 22, 30]);
        assert!(signals.iter().all(|(test, _, _)| *test == 1), "{signals:?}");
    }

    #[test]
    fn the_trend_sample_matches_the_captured_tests() {
        let signals = signals(
            "control_trend",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::all(),
        );
        assert_eq!(
            points(&signals, 3, "Location"),
            [7, 8, 9, 10, 11, 12, 19, 20, 21, 22, 23, 24]
        );
        assert_eq!(
            points(&signals, 2, "Location"),
            [9, 10, 11, 20, 21, 22, 23, 24]
        );
    }

    #[test]
    fn the_alternating_sample_matches_the_captured_tests() {
        let signals = signals(
            "control_alternating",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::all(),
        );
        assert_eq!(
            points(&signals, 4, "Location"),
            (15..=24).collect::<Vec<_>>()
        );
        assert_eq!(
            points(&signals, 7, "Location"),
            (15..=24).collect::<Vec<_>>()
        );
    }
}
