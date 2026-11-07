// SPDX-License-Identifier: MIT
//! E2E tests: they run the real binary, with real files on disk.
//!
//! It is the only level that proves the executable's behaviour (PLAN.md, 5.1,
//! level 3). Nothing here calls internal functions.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_FreePlots")
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fp-e2e-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos())
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

const CSV: &str = "weight;batch\n\
12,5;A\n13,1;A\n11,9;A\n12,8;A\n12,2;A\n\
13,4;A\n11,7;A\n12,0;A\n12,6;A\n12,9;A\n";

#[test]
fn analyze_exports_session_svg_and_project() {
    let dir = temp_dir("analyze");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");
    let svg = dir.join("plot.svg");
    let fpp = dir.join("data.fpp");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--svg",
            svg.to_str().expect("utf8"),
            "--project",
            fpp.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let session = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "Probability Plot: Normal",
        "Sample moments",
        "Mean (µ)",
        "Anderson–Darling",
        "p-value (Anderson–Darling)",
        "p-value method",
        "Plotting positions",
        "WARNINGS",
        "End of report",
    ] {
        assert!(
            session.contains(needle),
            "missing {needle:?} in Session:\n{session}"
        );
    }

    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    assert_eq!(svg_text.matches("<circle").count(), 10);
    assert!(
        svg_text.contains(">weight<"),
        "x axis should carry the column name"
    );
    assert!(svg_text.contains(">Percent<"), "missing probability axis");

    let project = fs::read_to_string(&fpp).expect("read project");
    // Schema 2 stores every worksheet; a single-sheet CLI export has one.
    assert!(project.contains("\"schema_version\": 2"), "{project}");
    assert!(project.contains("\"sheets\""), "{project}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lognormal_distribution_runs_end_to_end() {
    let dir = temp_dir("lognormal");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--distribution",
            "lognormal",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Probability Plot: Lognormal"), "{session}");
    assert!(session.contains("Location (µ)"), "{session}");
    assert!(session.contains("Scale (σ)"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn logistic_and_loglogistic_distributions_run_end_to_end() {
    let dir = temp_dir("logistic-families");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    for (distribution, title, parameter) in [
        ("logistic", "Probability Plot: Logistic", "Location (µ)"),
        ("loglogistic", "Probability Plot: Loglogistic", "Scale (α)"),
    ] {
        let out = Command::new(bin())
            .args([
                "analyze",
                csv.to_str().expect("utf8"),
                "--column",
                "weight",
                "--distribution",
                distribution,
            ])
            .output()
            .expect("run binary");
        assert!(
            out.status.success(),
            "failed for {distribution}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let session = String::from_utf8_lossy(&out.stdout);
        assert!(session.contains(title), "{distribution}: {session}");
        assert!(session.contains(parameter), "{distribution}: {session}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn weibull_distribution_runs_end_to_end() {
    let dir = temp_dir("weibull");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--distribution",
            "weibull",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Probability Plot: Weibull"), "{session}");
    assert!(session.contains("Shape (α)"), "{session}");
    assert!(session.contains("Scale (β)"), "{session}");
    assert!(session.contains("Weibull AD table"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn exponential_distribution_runs_end_to_end() {
    let dir = temp_dir("exponential");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--distribution",
            "exponential",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(
        session.contains("Probability Plot: Exponential"),
        "{session}"
    );
    assert!(session.contains("Mean (θ)"), "{session}");
    assert!(session.contains("exponential AD formula"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn multi_y_overlay_runs_end_to_end() {
    let dir = temp_dir("multiy");
    let csv = dir.join("two.csv");
    let svg = dir.join("two.svg");
    fs::write(
        &csv,
        "weight;height\n12.5;21.5\n13.1;22.1\n11.9;20.9\n12.8;21.8\n12.2;21.2\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--columns",
            "weight,height",
            "--layout",
            "overlay",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("column weight"), "{session}");
    assert!(session.contains("column height"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert_eq!(svg_text.matches("<circle").count(), 10);
    assert!(svg_text.contains(">weight<"), "legend weight missing");
    assert!(svg_text.contains(">height<"), "legend height missing");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn multi_y_separate_panels_is_the_default() {
    let dir = temp_dir("panels");
    let csv = dir.join("two.csv");
    let svg = dir.join("panels.svg");
    fs::write(
        &csv,
        "weight;height\n12.5;21.5\n13.1;22.1\n11.9;20.9\n12.8;21.8\n12.2;21.2\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--columns",
            "weight,height",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(
        svg_text.matches("<svg").count() >= 3,
        "expected nested panel SVGs"
    );
    assert!(svg_text.contains("column weight"), "weight panel title");
    assert!(svg_text.contains("column height"), "height panel title");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn grouped_overlay_runs_end_to_end() {
    let dir = temp_dir("grouped");
    let csv = dir.join("grouped.csv");
    let svg = dir.join("grouped.svg");
    fs::write(
        &csv,
        "weight;batch\n11.0;A\n11.5;A\n12.0;A\n13.0;B\n13.5;B\n14.0;B\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--group",
            "batch",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("weight (A)"), "{session}");
    assert!(session.contains("weight (B)"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.contains(">weight (A)<"), "legend A missing");
    assert!(svg_text.contains(">weight (B)<"), "legend B missing");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn display_symbols_omits_the_fitted_line() {
    let dir = temp_dir("symbols");
    let csv = dir.join("weights.csv");
    let svg = dir.join("plot.svg");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--display",
            "symbols",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert_eq!(svg_text.matches("<circle").count(), 10);
    assert!(
        !svg_text.contains("#c00000"),
        "fitted line should be hidden"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn descriptive_statistics_run_from_the_cli() {
    let dir = temp_dir("describe");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "describe",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Descriptive Statistics"), "{session}");
    assert!(session.contains("First quartile (Q1)"), "{session}");
    assert!(session.contains("Interquartile range"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ryan_joiner_normality_runs_from_the_cli() {
    let dir = temp_dir("rj");
    let csv = dir.join("normal.csv");
    fs::write(
        &csv,
        "x\n-1.2\n-0.8\n-0.5\n-0.2\n0.0\n0.1\n0.3\n0.6\n0.9\n1.1\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "normality",
            csv.to_str().expect("utf8"),
            "--column",
            "x",
            "--test",
            "rj",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Ryan–Joiner"), "{session}");
    assert!(session.contains("Monte Carlo RJ"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn kolmogorov_smirnov_normality_runs_from_the_cli() {
    let dir = temp_dir("ks");
    let csv = dir.join("normal.csv");
    fs::write(
        &csv,
        "x\n-1.2\n-0.8\n-0.5\n-0.2\n0.0\n0.1\n0.3\n0.6\n0.9\n1.1\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "normality",
            csv.to_str().expect("utf8"),
            "--column",
            "x",
            "--test",
            "ks",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Kolmogorov–Smirnov"), "{session}");
    assert!(session.contains("Monte Carlo KS"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn normality_test_runs_from_the_cli() {
    let dir = temp_dir("normality");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "normality",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Normality Test"), "{session}");
    assert!(session.contains("Anderson–Darling"), "{session}");
    assert!(session.contains("p-value"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn distribution_comparison_runs_from_the_cli() {
    let dir = temp_dir("distcompare");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "distcompare",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Distribution Comparison"), "{session}");
    for name in ["Normal", "Lognormal", "Weibull", "Exponential"] {
        assert!(session.contains(name), "{name} missing:\n{session}");
    }
    assert!(session.contains("AIC"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn capability_runs_from_the_cli() {
    let dir = temp_dir("capability");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");
    let svg = dir.join("capability.svg");

    let out = Command::new(bin())
        .args([
            "capability",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--lsl",
            "10",
            "--usl",
            "14",
            "--target",
            "11",
            "--confidence",
            "90",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Capability Analysis"), "{session}");
    assert!(session.contains("Cpk"), "{session}");
    assert!(session.contains("PPM"), "{session}");
    assert!(session.contains("Cpk 90% CI"), "{session}");
    assert!(session.contains("Cpm"), "{session}");
    assert!(fs::read_to_string(&svg).expect("svg").starts_with("<svg"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn box_cox_capability_runs_from_the_cli() {
    let dir = temp_dir("capability-box-cox");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "capability",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--lsl",
            "10",
            "--usl",
            "14",
            "--box-cox",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Normal (Box-Cox)"), "{session}");
    assert!(session.contains("Box-Cox λ"), "{session}");
    assert!(session.contains("Transformed specification"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn weibull_capability_runs_from_the_cli() {
    let dir = temp_dir("capability-weibull");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "capability",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--distribution",
            "weibull",
            "--lsl",
            "10",
            "--usl",
            "14",
            "--bootstrap",
            "50",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(
        session.contains("Capability Analysis: Weibull"),
        "{session}"
    );
    assert!(session.contains("PPM < LSL"), "{session}");
    assert!(session.contains("PPM total"), "{session}");
    assert!(session.contains("Bootstrap expected PPM"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn logistic_and_loglogistic_capability_run_from_the_cli() {
    let dir = temp_dir("capability-logistic-families");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    for (distribution, title) in [
        ("logistic", "Capability Analysis: Logistic"),
        ("loglogistic", "Capability Analysis: Loglogistic"),
    ] {
        let out = Command::new(bin())
            .args([
                "capability",
                csv.to_str().expect("utf8"),
                "--column",
                "weight",
                "--distribution",
                distribution,
                "--lsl",
                "10",
                "--usl",
                "14",
                "--bootstrap",
                "20",
            ])
            .output()
            .expect("run binary");
        assert!(
            out.status.success(),
            "failed for {distribution}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let session = String::from_utf8_lossy(&out.stdout);
        assert!(session.contains(title), "{distribution}: {session}");
        assert!(session.contains("PPM total"), "{distribution}: {session}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_phased_control_chart_runs_from_the_cli() {
    let dir = temp_dir("control-chart-phases");
    let csv = dir.join("phased.csv");
    fs::write(
        &csv,
        "weight;phase\n10;before\n10,2;before\n9,8;before\n10,1;before\n\
         20;after\n20,2;after\n19,8;after\n20,1;after\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "imr",
            "--phase",
            "phase",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Phases"), "{session}");
    assert!(session.contains("before"), "{session}");
    assert!(session.contains("after"), "{session}");

    // The phase column is only meaningful for the variable charts.
    let rejected = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "ewma",
            "--phase",
            "phase",
        ])
        .output()
        .expect("run binary");
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr).contains("--phase"),
        "{}",
        String::from_utf8_lossy(&rejected.stderr)
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn control_chart_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("control-chart");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");
    let svg = dir.join("imr.svg");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "imr",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Control Chart: I–MR"), "{session}");
    assert!(session.contains("Control limits"), "{session}");
    assert!(session.contains("Location UCL"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    assert!(svg_text.contains(">Individuals<"), "{svg_text}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn control_chart_test_selection_runs_from_the_cli() {
    let dir = temp_dir("control-chart-tests");
    let csv = dir.join("weights.csv");
    // A single spike: test 1 fires, and the run tests stay quiet.
    let mut rows = String::from("weight\n");
    for index in 0..20 {
        let value = if index == 12 {
            20.0
        } else {
            12.0 + (index % 2) as f64 * 0.2
        };
        rows.push_str(&format!("{value}\n"));
    }
    fs::write(&csv, rows).expect("write csv");

    let run = |extra: &[&str]| -> String {
        let out = Command::new(bin())
            .args([
                "controlchart",
                csv.to_str().expect("utf8"),
                "--column",
                "weight",
                "--chart",
                "imr",
            ])
            .args(extra)
            .output()
            .expect("run binary");
        assert!(
            out.status.success(),
            "failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let session = run(&["--tests", "1"]);
    assert!(
        session.contains("point outside a control limit"),
        "{session}"
    );
    assert!(session.contains("tests 1, subgroup"), "{session}");

    let session = run(&["--tests", "none"]);
    assert!(session.contains("No out-of-control points"), "{session}");

    let session = run(&["--tests", "1:8", "--sensitivity", "2=8"]);
    assert!(session.contains("test 2 K = 8"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn xbar_r_chart_runs_from_the_cli() {
    let dir = temp_dir("control-chart-xbar");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "xbar-r",
            "--subgroup",
            "2",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Control Chart: X̄–R"), "{session}");
    assert!(session.contains("Subgroups used"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn run_chart_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("run-chart");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");
    let svg = dir.join("run.svg");

    let out = Command::new(bin())
        .args([
            "runchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Run Chart"), "{session}");
    assert!(session.contains("About the median"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn t_tests_run_from_the_cli() {
    let dir = temp_dir("t-tests");
    let csv = dir.join("pairs.csv");
    fs::write(&csv, "before;after\n1;2\n2;4\n3;6\n4;8\n5;10\n").expect("write csv");

    let one = Command::new(bin())
        .args([
            "ttest1",
            csv.to_str().expect("utf8"),
            "--column",
            "before",
            "--mean",
            "3",
            "--alternative",
            "less",
        ])
        .output()
        .expect("run binary");
    assert!(
        one.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&one.stderr)
    );
    let session = String::from_utf8_lossy(&one.stdout);
    assert!(session.contains("1-Sample t"), "{session}");
    assert!(session.contains("p-value"), "{session}");
    assert!(session.contains("Upper bound"), "{session}");

    let two = Command::new(bin())
        .args([
            "ttest2",
            csv.to_str().expect("utf8"),
            "--columns",
            "before,after",
            "--pooled",
        ])
        .output()
        .expect("run binary");
    assert!(
        two.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&two.stderr)
    );
    let session = String::from_utf8_lossy(&two.stdout);
    assert!(session.contains("2-Sample t"), "{session}");
    assert!(session.contains("Pooled"), "{session}");

    let paired = Command::new(bin())
        .args([
            "ttestpaired",
            csv.to_str().expect("utf8"),
            "--columns",
            "before,after",
        ])
        .output()
        .expect("run binary");
    assert!(
        paired.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&paired.stderr)
    );
    let session = String::from_utf8_lossy(&paired.stdout);
    assert!(session.contains("Paired t"), "{session}");
    assert!(session.contains("Pairs"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn lognormal_reliability_runs_from_the_cli() {
    let dir = temp_dir("lognormal-rel");
    let csv = dir.join("survival.csv");
    fs::write(&csv, "time;status\n1;1\n2;1\n3;1\n4;1\n5;0\n8;0\n10;0\n").expect("write csv");
    let out = Command::new(bin())
        .args([
            "lognormalrel",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--censor",
            "status",
            "--censor-value",
            "0",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Lognormal Reliability"), "{session}");
    assert!(session.contains("B10 life"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn exponential_reliability_runs_from_the_cli() {
    let dir = temp_dir("exponential-rel");
    let csv = dir.join("survival.csv");
    fs::write(&csv, "time;status\n1;1\n2;1\n3;1\n4;1\n10;0\n").expect("write csv");
    let out = Command::new(bin())
        .args([
            "exporeliability",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--censor",
            "status",
            "--censor-value",
            "0",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Exponential Reliability"), "{session}");
    assert!(session.contains("B10 life"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn censored_weibull_runs_from_the_cli() {
    let dir = temp_dir("weibull-rel");
    let csv = dir.join("survival.csv");
    fs::write(
        &csv,
        "time;status\n1;1\n2;1\n3;1\n4;1\n5;0\n6;0\n8;0\n10;0\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "weibullrel",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--censor",
            "status",
            "--censor-value",
            "0",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Weibull Reliability"), "{session}");
    assert!(session.contains("B10 life"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fractional_design_and_main_effects_run_from_the_cli() {
    let dir = temp_dir("fractional");
    let design = dir.join("frac.csv");
    let out = Command::new(bin())
        .args([
            "fracdesign",
            design.to_str().expect("utf8"),
            "--base",
            "A,B,C",
            "--generators",
            "D=ABC",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&design).expect("design");
    assert_eq!(text.lines().count(), 9);

    let data = dir.join("frac_y.csv");
    fs::write(
        &data,
        "A;B;C;D;y\n-1;-1;-1;-1;0\n1;-1;-1;1;10\n-1;1;-1;1;6\n1;1;-1;-1;6\n-1;-1;1;1;8\n1;-1;1;-1;8\n-1;1;1;-1;12\n1;1;1;1;24\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "doe",
            data.to_str().expect("utf8"),
            "--response",
            "y",
            "--factors",
            "A,B,C,D",
            "--main-effects",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Fractional Factorial"), "{session}");
    assert!(session.contains("Aliasing"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn histogram_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("histogram");
    let csv = dir.join("hist.csv");
    fs::write(&csv, "x\n1\n2\n2\n3\n3\n3\n4\n4\n5\n").expect("write csv");
    let svg = dir.join("hist.svg");
    let out = Command::new(bin())
        .args([
            "histogram",
            csv.to_str().expect("utf8"),
            "--column",
            "x",
            "--bins",
            "5",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Histogram"), "{session}");
    assert!(session.contains("Bin width"), "{session}");
    assert!(fs::read_to_string(&svg).expect("svg").starts_with("<svg"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn boxplot_and_scatter_run_from_the_cli() {
    let dir = temp_dir("graphs");
    let csv = dir.join("graphs.csv");
    fs::write(&csv, "x;y;g\n1;2;A\n2;4;A\n3;6;A\n4;8;B\n5;10;B\n6;12;B\n").expect("write csv");
    let box_svg = dir.join("box.svg");
    let out = Command::new(bin())
        .args([
            "boxplot",
            csv.to_str().expect("utf8"),
            "--column",
            "y",
            "--by",
            "g",
            "--svg",
            box_svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Boxplot"), "{session}");
    assert!(
        fs::read_to_string(&box_svg)
            .expect("box svg")
            .starts_with("<svg")
    );

    let scatter_svg = dir.join("scatter.svg");
    let out = Command::new(bin())
        .args([
            "scatter",
            csv.to_str().expect("utf8"),
            "--x",
            "x",
            "--y",
            "y",
            "--svg",
            scatter_svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Scatterplot"), "{session}");
    assert!(session.contains("Pearson r"), "{session}");
    assert!(
        fs::read_to_string(&scatter_svg)
            .expect("scatter svg")
            .starts_with("<svg")
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn response_surface_and_ccd_run_from_the_cli() {
    let dir = temp_dir("rsm");
    let csv = dir.join("rsm.csv");
    fs::write(
        &csv,
        "A;B;y\n-1;-1;8\n-1;0;9\n-1;1;10\n0;-1;9\n0;0;10\n0;1;11\n1;-1;10\n1;0;11\n1;1;12\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "rsm",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--factors",
            "A,B",
            "--optimize",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Response Surface"), "{session}");
    assert!(session.contains("A^2"), "{session}");
    assert!(session.contains("Stationary point"), "{session}");

    let design = dir.join("ccd.csv");
    let out = Command::new(bin())
        .args([
            "ccd",
            design.to_str().expect("utf8"),
            "--factors",
            "A,B",
            "--center-points",
            "4",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&design).expect("read design");
    assert!(text.contains("StdOrder,RunOrder,A,B"), "{text}");
    assert_eq!(text.lines().count(), 13);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn nested_gage_rr_runs_from_the_cli() {
    let dir = temp_dir("gage-rr-nested");
    let csv = dir.join("gage_nested.csv");
    fs::write(
        &csv,
        "m;part;operator\n1;P1;O1\n2;P1;O1\n3;P2;O1\n4;P2;O1\n5;P3;O2\n6;P3;O2\n7;P4;O2\n8;P4;O2\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "gagerr",
            csv.to_str().expect("utf8"),
            "--measurement",
            "m",
            "--part",
            "part",
            "--operator",
            "operator",
            "--type",
            "nested",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Nested Gage R&R"), "{session}");
    assert!(session.contains("Part (operator)"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn doe_design_is_written_from_the_cli() {
    let dir = temp_dir("doe-design");
    let csv = dir.join("design.csv");

    let out = Command::new(bin())
        .args([
            "doedesign",
            csv.to_str().expect("utf8"),
            "--factors",
            "A,B",
            "--replicates",
            "2",
            "--center-points",
            "1",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&csv).expect("read design");
    assert!(text.contains("StdOrder,RunOrder,A,B"), "{text}");
    assert_eq!(text.lines().count(), 10);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn factorial_doe_runs_from_the_cli() {
    let dir = temp_dir("doe");
    let csv = dir.join("doe.csv");
    fs::write(
        &csv,
        "A;B;y\n-1;-1;6\n-1;-1;6\n1;-1;10\n1;-1;10\n-1;1;12\n-1;1;12\n1;1;18\n1;1;18\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "doe",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--factors",
            "A,B",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("2-Level Factorial"), "{session}");
    assert!(session.contains("Effect"), "{session}");
    assert!(session.contains("A*B"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn xlsx_export_writes_a_workbook() {
    let dir = temp_dir("xlsx");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");
    let book = dir.join("weights.xlsx");
    let out = Command::new(bin())
        .args([
            "xlsx",
            csv.to_str().expect("utf8"),
            "--output",
            book.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = fs::read(&book).expect("workbook");
    assert_eq!(&bytes[0..4], b"PK\x03\x04");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn fractional_alias_table_runs_from_the_cli() {
    let out = Command::new(bin())
        .args(["doealias", "--base", "A,B,C", "--generators", "D=ABC"])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Alias groups"), "{session}");
    assert!(session.contains("A = B = C = D"), "{session}");
}

#[test]
fn ewma_chart_and_blocked_design_run_from_the_cli() {
    let dir = temp_dir("ewma-blocks");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "ewma",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Control Chart: EWMA"), "{session}");

    let design = dir.join("design.csv");
    let out = Command::new(bin())
        .args([
            "doedesign",
            design.to_str().expect("utf8"),
            "--factors",
            "A,B",
            "--replicates",
            "2",
            "--block-size",
            "2",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&design).expect("design");
    assert!(text.contains("Block"), "{text}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn gage_type1_runs_from_the_cli() {
    let dir = temp_dir("gage-type1");
    let csv = dir.join("type1.csv");
    fs::write(&csv, "m\n10.1\n10.2\n10.0\n10.3\n10.1\n10.2\n").expect("write csv");
    let out = Command::new(bin())
        .args([
            "gagetype1",
            csv.to_str().expect("utf8"),
            "--measurement",
            "m",
            "--reference",
            "10",
            "--lsl",
            "9",
            "--usl",
            "11",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Gage Type 1"), "{session}");
    assert!(session.contains("%Tolerance"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn gage_rr_runs_from_the_cli() {
    let dir = temp_dir("gage-rr");
    let csv = dir.join("gage.csv");
    fs::write(
        &csv,
        "m;part;operator\n1;A;O1\n2;A;O1\n2;A;O2\n3;A;O2\n4;B;O1\n5;B;O1\n5;B;O2\n6;B;O2\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "gagerr",
            csv.to_str().expect("utf8"),
            "--measurement",
            "m",
            "--part",
            "part",
            "--operator",
            "operator",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Gage R&R"), "{session}");
    assert!(session.contains("%Gage R&R"), "{session}");
    assert!(session.contains("Repeatability"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn kaplan_meier_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("kaplan-meier");
    let csv = dir.join("survival.csv");
    fs::write(&csv, "time;status\n1;1\n2;0\n3;1\n4;1\n5;0\n").expect("write csv");
    let svg = dir.join("km.svg");

    let out = Command::new(bin())
        .args([
            "reliability",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--censor",
            "status",
            "--censor-value",
            "0",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Kaplan–Meier"), "{session}");
    assert!(session.contains("Median survival"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn categorical_regression_runs_from_the_cli() {
    let dir = temp_dir("regression-categorical");
    let csv = dir.join("reg.csv");
    fs::write(
        &csv,
        "x;group;y\n1;A;4\n2;A;6\n3;A;8\n4;B;14\n5;B;16\n6;B;18\n",
    )
    .expect("write csv");
    let out = Command::new(bin())
        .args([
            "regress",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--predictors",
            "x,group",
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Regression"), "{session}");
    assert!(session.contains("group[B]"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn anova_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("anova");
    let csv = dir.join("anova.csv");
    fs::write(&csv, "y;g\n1;A\n2;A\n3;B\n4;B\n5;C\n6;C\n").expect("write csv");
    let svg = dir.join("residuals.svg");

    let out = Command::new(bin())
        .args([
            "anova",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--factor",
            "g",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("One-Way ANOVA"), "{session}");
    assert!(session.contains("Bonferroni"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn regression_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("regression");
    let csv = dir.join("reg.csv");
    fs::write(
        &csv,
        "x1;x2;y\n1;2;6\n2;1;5\n3;4;12\n4;3;11\n5;6;18\n6;5;17\n",
    )
    .expect("write csv");
    let svg = dir.join("residuals.svg");

    let out = Command::new(bin())
        .args([
            "regress",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--predictors",
            "x1,x2",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Regression"), "{session}");
    assert!(session.contains("Regression equation"), "{session}");
    assert!(session.contains("SE Coef"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn stepwise_regression_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("stepwise");
    let csv = dir.join("step.csv");
    fs::write(
        &csv,
        "x1;x2;y\n\
         0;0.769231;1.410456\n1;-1.188811;4.198339\n2;0.853147;5.105444\n\
         3;-1.104895;5.594605\n4;0.937063;8.019128\n5;-1.020979;11.014304\n\
         6;1.020979;13.465211\n7;-0.937063;14.883670\n8;1.104895;16.664885\n\
         9;-0.853147;18.692888\n10;1.188811;21.334876\n11;-0.769231;23.616195\n",
    )
    .expect("write csv");
    let svg = dir.join("residuals.svg");

    let out = Command::new(bin())
        .args([
            "stepwise",
            csv.to_str().expect("utf8"),
            "--response",
            "y",
            "--predictors",
            "x1,x2",
            "--method",
            "forward",
            "--criterion",
            "p",
            "--alpha-enter",
            "0.05",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Stepwise Regression"), "{session}");
    assert!(session.contains("Regression equation"), "{session}");
    assert!(session.contains("Selected"), "{session}");
    let svg_text = fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.starts_with("<svg"), "invalid SVG");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn proportion_and_variance_tests_run_from_the_cli() {
    let dir = temp_dir("prop-var");
    let csv = dir.join("binary.csv");
    fs::write(&csv, "a;b\n1;1\n1;1\n1;0\n0;0\n0;0\n0;0\n").expect("write csv");

    for (command, extra, needle) in [
        (
            "prop1",
            vec!["--column", "a", "--p0", "0.5"],
            "1 Proportion",
        ),
        ("prop2", vec!["--columns", "a,b"], "2 Proportions"),
        (
            "var1",
            vec!["--column", "a", "--variance", "0.25"],
            "1 Variance",
        ),
        ("var2", vec!["--columns", "a,b"], "2 Variances"),
    ] {
        let mut args = vec![command, csv.to_str().expect("utf8")];
        args.extend(extra);
        let out = Command::new(bin())
            .args(&args)
            .output()
            .expect("run binary");
        assert!(
            out.status.success(),
            "failed for {command}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let session = String::from_utf8_lossy(&out.stdout);
        assert!(session.contains(needle), "{command}: {session}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn attribute_charts_run_from_the_cli() {
    let dir = temp_dir("attribute-chart");
    let csv = dir.join("defects.csv");
    fs::write(
        &csv,
        "defects;size\n2;100\n4;100\n3;100\n5;100\n1;100\n4;100\n",
    )
    .expect("write csv");

    let p = Command::new(bin())
        .args([
            "attrchart",
            csv.to_str().expect("utf8"),
            "--column",
            "defects",
            "--chart",
            "p",
            "--sizes",
            "size",
        ])
        .output()
        .expect("run binary");
    assert!(
        p.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&p.stderr)
    );
    let session = String::from_utf8_lossy(&p.stdout);
    assert!(session.contains("P Chart"), "{session}");
    assert!(session.contains("Center line"), "{session}");

    let c = Command::new(bin())
        .args([
            "attrchart",
            csv.to_str().expect("utf8"),
            "--column",
            "defects",
            "--chart",
            "c",
        ])
        .output()
        .expect("run binary");
    assert!(
        c.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&c.stderr)
    );
    let session = String::from_utf8_lossy(&c.stdout);
    assert!(session.contains("C Chart"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn describe_by_prints_one_block_per_group() {
    let dir = temp_dir("describe-by");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "describe",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--by",
            "batch",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("batch = A"), "{session}");
    assert!(session.contains("Descriptive Statistics"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn monte_carlo_p_value_remains_available() {
    let dir = temp_dir("mc");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "analyze",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--p-value",
            "mc",
            "--reps",
            "50",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("p-value (Monte Carlo)"), "{session}");
    assert!(session.contains("50 replications"), "{session}");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn nonexistent_file_fails_with_message() {
    let out = Command::new(bin())
        .args(["analyze", "/does/not/exist.csv"])
        .output()
        .expect("run binary");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"), "stderr: {stderr}");
    assert!(stderr.contains("exist.csv"), "stderr: {stderr}");
}

#[test]
fn version_and_doctor_respond() {
    let v = Command::new(bin())
        .arg("--version")
        .output()
        .expect("version");
    assert!(v.status.success());
    assert!(String::from_utf8_lossy(&v.stdout).contains("FreePlots"));

    let d = Command::new(bin()).arg("doctor").output().expect("doctor");
    assert!(d.status.success());
    let text = String::from_utf8_lossy(&d.stdout);
    assert!(text.contains("Data directory"), "{text}");
    assert!(
        text.contains("portable") || text.contains("fallback"),
        "{text}"
    );
}

#[test]
fn unknown_command_returns_error() {
    let out = Command::new(bin()).arg("bar").output().expect("run");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown"));
}

#[test]
fn cusum_chart_accepts_custom_k_and_h() {
    let dir = temp_dir("control-chart-cusum");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "cusum",
            "--cusum-k",
            "1",
            "--cusum-h",
            "4",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Control Chart: CUSUM"), "{session}");
    assert!(session.contains("k = 1σ"), "{session}");
    assert!(session.contains("h = 4σ"), "{session}");
    assert!(
        session.contains("Reference value (k, in sigma units)"),
        "{session}"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_zero_cusum_reference_value_is_rejected() {
    let dir = temp_dir("control-chart-cusum-invalid");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "cusum",
            "--cusum-k",
            "0",
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success(), "a zero k must fail");
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("reference value"), "{error}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ewma_chart_accepts_custom_lambda_and_limits() {
    let dir = temp_dir("control-chart-ewma");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "ewma",
            "--ewma-lambda",
            "0.4",
            "--ewma-l",
            "2.5",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("Control Chart: EWMA"), "{session}");
    assert!(session.contains("λ = 0.4"), "{session}");
    assert!(session.contains("L = 2.5σ"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_out_of_range_ewma_lambda_is_rejected() {
    let dir = temp_dir("control-chart-ewma-invalid");
    let csv = dir.join("weights.csv");
    fs::write(&csv, CSV).expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "weight",
            "--chart",
            "ewma",
            "--ewma-lambda",
            "1.5",
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success(), "lambda above one must fail");
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("lambda"), "{error}");
    fs::remove_dir_all(&dir).ok();
}

const STEP_STRESS: &str = "\
unit;duration;stress;event
U1;7,5;300;1
U2;20,0;300;0
U2;3,2;350;1
U3;12,4;300;1
U4;20,0;300;0
U4;40,0;350;0
U5;5,1;300;1
U6;20,0;300;0
U6;9,8;350;1
U7;16,3;300;1
U8;2,4;300;1
U9;20,0;300;0
U9;14,7;350;1
U10;8,8;300;1
U11;20,0;300;0
U11;40,0;350;0
U12;6,2;300;1
";

#[test]
fn step_stress_alt_runs_from_the_cli_and_exports_svg() {
    let dir = temp_dir("stepstress");
    let csv = dir.join("stepstress.csv");
    fs::write(&csv, STEP_STRESS).expect("write csv");
    let svg = dir.join("stepstress.svg");

    let out = Command::new(bin())
        .args([
            "stepstress",
            csv.to_str().expect("utf8"),
            "--duration",
            "duration",
            "--stress",
            "stress",
            "--unit",
            "unit",
            "--event",
            "event",
            "--use-stress",
            "300",
            "--distribution",
            "weibull",
            "--model",
            "arrhenius",
            "--svg",
            svg.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "Step-Stress ALT",
        "cumulative-exposure",
        "Life at use",
        "B10 at use",
        "Shape (Weibull)",
    ] {
        assert!(session.contains(needle), "missing {needle}: {session}");
    }
    assert!(svg.exists(), "svg not written");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn step_stress_alt_rejects_a_missing_unit_column() {
    let dir = temp_dir("stepstress-invalid");
    let csv = dir.join("stepstress.csv");
    fs::write(&csv, STEP_STRESS).expect("write csv");

    let out = Command::new(bin())
        .args([
            "stepstress",
            csv.to_str().expect("utf8"),
            "--duration",
            "duration",
            "--stress",
            "stress",
            "--event",
            "event",
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("--unit"), "{error}");
    fs::remove_dir_all(&dir).ok();
}

const COMPETING: &str = "\
time;stress;censor;mode
12,4;300;0;wear
18,9;350;0;corrosion
25,1;300;0;wear
31,7;350;0;corrosion
44,2;300;0;wear
9,3;350;0;corrosion
57,8;300;0;wear
21,6;350;0;corrosion
38,5;300;0;wear
48,1;350;0;corrosion
15,2;300;0;wear
27,4;350;0;corrosion
66,3;300;0;wear
11,8;350;0;corrosion
33,9;300;0;wear
52,7;350;0;corrosion
19,5;300;0;wear
41,3;350;0;corrosion
28,6;300;0;wear
60,2;350;0;corrosion
";

#[test]
fn accelerated_life_fits_competing_risks() {
    let dir = temp_dir("competing");
    let csv = dir.join("competing.csv");
    fs::write(&csv, COMPETING).expect("write csv");

    let out = Command::new(bin())
        .args([
            "alt",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--stress",
            "stress",
            "--mode",
            "mode",
            "--horizon",
            "40",
            "--use-stress",
            "300",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    for needle in ["competing risks", "wear", "corrosion", "CIF at horizon"] {
        assert!(session.contains(needle), "missing {needle}: {session}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_competing_horizon_without_a_mode_is_rejected() {
    let dir = temp_dir("competing-invalid");
    let csv = dir.join("competing.csv");
    fs::write(&csv, COMPETING).expect("write csv");

    let out = Command::new(bin())
        .args([
            "alt",
            csv.to_str().expect("utf8"),
            "--time",
            "time",
            "--stress",
            "stress",
            "--horizon",
            "40",
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("--mode"), "{error}");
    fs::remove_dir_all(&dir).ok();
}

const DESIRABILITY: &str = "\
A;B;Strength;Yield
-1;-1;13,70;62,70
1;-1;25,30;74,10
-1;1;16,70;68,70
1;1;23,30;75,10
-1;0;15,40;65,30
1;0;25,20;75,00
0;-1;20,80;67,00
0;1;21,20;73,00
0;0;20,50;70,40
0;0;20,60;70,20
";

#[test]
fn the_response_optimizer_runs_from_the_cli() {
    let dir = temp_dir("desirability");
    let csv = dir.join("desirability.csv");
    fs::write(&csv, DESIRABILITY).expect("write csv");

    let out = Command::new(bin())
        .args([
            "desirability",
            csv.to_str().expect("utf8"),
            "--factors",
            "A,B",
            "--response",
            "Strength:maximize:10:30",
            "--response",
            "Yield:target:60:100:target=75:importance=2",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "Response Optimizer",
        "Combined desirability",
        "Strength",
        "Maximize",
        "Target",
    ] {
        assert!(session.contains(needle), "missing {needle}: {session}");
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_invalid_desirability_response_is_rejected() {
    let dir = temp_dir("desirability-invalid");
    let csv = dir.join("desirability.csv");
    fs::write(&csv, DESIRABILITY).expect("write csv");

    let out = Command::new(bin())
        .args([
            "desirability",
            csv.to_str().expect("utf8"),
            "--factors",
            "A,B",
            "--response",
            "Strength:sideways:10:30",
        ])
        .output()
        .expect("run binary");

    assert!(!out.status.success());
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("maximize"), "{error}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn unequal_subgroups_come_from_a_column() {
    let dir = temp_dir("subgroups");
    let csv = dir.join("subgroups.csv");
    fs::write(
        &csv,
        "measure;lot\n10,1;L1\n10,3;L1\n9,9;L1\n10,2;L2\n10,0;L2\n\
         9,8;L3\n10,4;L3\n10,1;L3\n10,2;L3\n10,3;L4\n10,1;L4\n10,0;L4\n\
         9,9;L5\n10,2;L5\n",
    )
    .expect("write csv");

    let out = Command::new(bin())
        .args([
            "controlchart",
            csv.to_str().expect("utf8"),
            "--column",
            "measure",
            "--chart",
            "xbar-r",
            "--subgroup-column",
            "lot",
        ])
        .output()
        .expect("run binary");

    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let session = String::from_utf8_lossy(&out.stdout);
    assert!(session.contains("variable sizes (2–4)"), "{session}");
    assert!(session.contains("one point per subgroup"), "{session}");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn date_tools_add_difference_and_shift_columns() {
    let dir = temp_dir("dates");
    let csv = dir.join("dates.csv");
    fs::write(
        &csv,
        "start;end\n2024-01-01;2024-01-31\n2024-02-29;2024-03-01\n2025-01-01;2024-12-31\n",
    )
    .expect("write csv");

    let diff_csv = dir.join("difference.csv");
    let out = Command::new(bin())
        .args([
            "datediff",
            csv.to_str().expect("utf8"),
            "--first",
            "start",
            "--second",
            "end",
            "--output",
            diff_csv.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("mean=10.0000"), "{stdout}");
    assert!(stdout.contains("min=-1"), "{stdout}");
    let written = fs::read_to_string(&diff_csv).expect("read output");
    assert!(written.starts_with("start,end,difference"), "{written}");
    assert!(written.contains("30"), "{written}");

    let shift_csv = dir.join("shifted.csv");
    let out = Command::new(bin())
        .args([
            "dateadd",
            csv.to_str().expect("utf8"),
            "--column",
            "start",
            "--days",
            "30",
            "--name",
            "due",
            "--output",
            shift_csv.to_str().expect("utf8"),
        ])
        .output()
        .expect("run binary");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = fs::read_to_string(&shift_csv).expect("read output");
    assert!(written.contains("due"), "{written}");
    assert!(written.contains("2024-01-31"), "{written}");
    assert!(written.contains("2024-03-30"), "{written}");
    fs::remove_dir_all(&dir).ok();
}

// The renderer selection happens before any window is created, so a bad value
// is rejected in a headless test. Painting itself is checked by
// `scripts/windows-gui-smoke.ps1` on a real Windows desktop.
#[cfg(feature = "desktop")]
#[test]
fn an_unknown_renderer_is_rejected_before_the_window_opens() {
    let out = Command::new(bin())
        .args(["--renderer", "banana"])
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid --renderer: banana"), "{stderr}");
}

#[cfg(feature = "desktop")]
#[test]
fn a_renderer_without_a_value_is_rejected() {
    let out = Command::new(bin())
        .arg("--renderer")
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--renderer requires glow or wgpu"),
        "{stderr}"
    );
}

#[cfg(feature = "desktop")]
#[test]
fn a_renderer_variable_with_an_unknown_value_is_rejected() {
    let out = Command::new(bin())
        .env("FREEPLOTS_RENDERER", "banana")
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid FREEPLOTS_RENDERER: banana"),
        "{stderr}"
    );
}

// An option that is present without its value used to be treated as absent
// when it was the last argument, so the command silently ran with its default
// instead of telling the caller that the value was missing.
#[test]
fn an_optional_option_at_the_end_without_a_value_is_rejected() {
    let dir = temp_dir("dangling-option");
    let csv = dir.join("data.csv");
    fs::write(&csv, "a,b,g\n1,2,A\n2,4,A\n3,6,B\n4,8,B\n").expect("write csv");
    let out_csv = dir.join("out.csv");
    let check = |args: &[&str], message: &str| {
        let out = Command::new(bin()).args(args).output().expect("run binary");
        assert_eq!(out.status.code(), Some(1), "args: {args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(message), "args: {args:?}\nstderr: {stderr}");
    };
    check(
        &[
            "subset",
            csv.to_str().expect("utf8"),
            out_csv.to_str().expect("utf8"),
            "--column",
            "g",
            "--op",
        ],
        "--op requires a value",
    );
    check(
        &["sampleplan", "--aql", "1", "--ltpd", "5", "--lot"],
        "--lot requires a value",
    );
    check(
        &["occurve", "--sample", "20", "--accept", "1", "--svg"],
        "--svg requires a path",
    );
    fs::remove_dir_all(&dir).ok();
}
