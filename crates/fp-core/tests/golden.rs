//! Golden files: the SVG and the Session text are frozen and any change has to
//! show up as a diff in a PR.
//!
//! Update: `UPDATE_GOLDEN=1 cargo test -p fp-core --test golden`.

use std::path::{Path, PathBuf};

fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

/// Compares with the golden file, or writes it if `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("create dir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "golden {} unreadable ({e}). Generate with UPDATE_GOLDEN=1 cargo test -p fp-core --test golden",
            path.display()
        )
    });
    // Git may check text files out with CRLF on Windows; compare canonical LF.
    let expected = expected.replace("\r\n", "\n");
    let actual = actual.replace("\r\n", "\n");
    if expected != actual {
        let first_diff = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "golden {name} diverged at line {first_diff}.\n\
             Expected: {:?}\nGot:      {:?}",
            expected.lines().nth(first_diff).unwrap_or("<end>"),
            actual.lines().nth(first_diff).unwrap_or("<end>")
        );
    }
}

fn fixed_sample() -> fp_core::Sample {
    // 20 values with known standard deviation; fixed so the golden stays stable.
    fp_core::Sample::new(
        vec![
            10.2, 11.4, 9.8, 12.1, 10.9, 11.0, 9.5, 10.4, 11.8, 10.1, 12.4, 9.9, 10.7, 11.2, 10.0,
            11.6, 10.5, 9.7, 11.1, 10.3,
        ],
        fp_core::MissingPolicy::Drop,
    )
    .expect("valid sample")
}

#[test]
fn probability_plot_svg() {
    use fp_core::dists::Normal;
    use fp_core::plot::{PlotSpec, PlottingPosition, normal_probability_plot, render_svg};

    let sample = fixed_sample();
    let dist = Normal::fit_moments(&sample).expect("fit");
    let plot = normal_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::normal_probability(&plot, &dist, "Probability Plot: Normal");
    assert_golden("normal_pp.svg", &render_svg(&spec));
}

#[test]
fn lognormal_probability_plot_svg() {
    use fp_core::dists::Lognormal;
    use fp_core::plot::{PlotSpec, PlottingPosition, lognormal_probability_plot, render_svg};

    let sample = fp_core::Sample::new(
        vec![
            1.20, 1.50, 1.10, 2.00, 1.80, 1.30, 1.60, 1.40, 1.90, 1.70, 2.20, 1.00, 1.45, 1.55,
            1.25, 1.75, 1.35, 1.65, 1.15, 1.85,
        ],
        fp_core::MissingPolicy::Drop,
    )
    .expect("valid sample");
    let dist = Lognormal::fit_moments(&sample).expect("fit");
    let plot = lognormal_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::lognormal_probability(&plot, &dist, "Probability Plot: Lognormal");
    assert_golden("lognormal_pp.svg", &render_svg(&spec));
}

#[test]
fn weibull_probability_plot_svg() {
    use fp_core::dists::Weibull;
    use fp_core::plot::{PlotSpec, PlottingPosition, render_svg, weibull_probability_plot};

    let sample = fp_core::Sample::new(
        vec![
            1.20, 1.50, 1.10, 2.00, 1.80, 1.30, 1.60, 1.40, 1.90, 1.70, 2.20, 1.00, 1.45, 1.55,
            1.25, 1.75, 1.35, 1.65, 1.15, 1.85,
        ],
        fp_core::MissingPolicy::Drop,
    )
    .expect("valid sample");
    let dist = Weibull::fit_mle(&sample).expect("fit");
    let plot = weibull_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::weibull_probability(&plot, &dist, "Probability Plot: Weibull");
    assert_golden("weibull_pp.svg", &render_svg(&spec));
}

#[test]
fn exponential_probability_plot_svg() {
    use fp_core::dists::Exponential;
    use fp_core::plot::{PlotSpec, PlottingPosition, exponential_probability_plot, render_svg};

    let sample = fp_core::Sample::new(
        vec![
            0.2532, 0.7796, 1.3353, 1.9237, 2.5489, 3.2158, 3.9304, 4.7000, 5.5339, 6.4436, 7.4444,
            8.5567, 9.8083, 11.2393, 12.9098, 14.9165, 17.4297, 20.7944, 25.9027, 36.8888,
        ],
        fp_core::MissingPolicy::Drop,
    )
    .expect("valid sample");
    let dist = Exponential::fit_mle(&sample).expect("fit");
    let plot =
        exponential_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::exponential_probability(&plot, &dist, "Probability Plot: Exponential");
    assert_golden("exponential_pp.svg", &render_svg(&spec));
}

#[test]
fn session_text() {
    use fp_core::report::AnalysisResult;

    let mut r = AnalysisResult::new(
        "Probability Plot: Normal — column C1",
        "Sample moments (n−1)",
    );
    r.n = 20;
    r.excluded = 0;
    r.parameters = vec![("Mean (µ)", 10.735), ("Standard deviation (σ)", 0.786)];
    r.statistics = vec![("Median", 10.65), ("Minimum", 9.5), ("Maximum", 12.4)];
    r.gof = vec![
        ("Anderson–Darling (A²)", "0.312000".to_owned()),
        ("p-value (Anderson–Darling)", "0.522179".to_owned()),
    ];
    r.notes = vec![(
        "Plotting positions".to_owned(),
        "Benard (median ranks) — (i − 0.3) / (n + 0.4)".to_owned(),
    )];
    r.warnings = vec!["small sample (n = 20 < 30): interpret with caution".to_owned()];
    assert_golden("session.txt", &r.to_session_text());
}
