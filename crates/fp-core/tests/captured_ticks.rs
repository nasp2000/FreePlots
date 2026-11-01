//! Tick labels frozen from the captured plots.
//!
//! The captured run sets are recorded in `validation/results.md`: normal/lognormal
//! `1 … 99 %` (0.1/99.9 at n = 200) and Weibull/exponential `1, 2, 3, 5 … 99 %`.
//! These tests keep the engine's axis in step with those captures.

use fp_core::data::{MissingPolicy, Sample};
use fp_core::dists::{Normal, Weibull};
use fp_core::plot::{
    PlotSpec, PlottingPosition, normal_probability_plot, weibull_probability_plot,
};

fn increasing(n: usize, start: f64, step: f64) -> Sample {
    let values: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
    Sample::new(values, MissingPolicy::Drop).expect("sample")
}

fn labels(spec: &PlotSpec) -> Vec<&str> {
    spec.y_axis.ticks.iter().map(|t| t.label.as_str()).collect()
}

#[test]
fn normal_n8_uses_the_captured_1_to_99_set() {
    let sample = increasing(8, 10.0, 1.0);
    let dist = Normal::fit_moments(&sample).expect("fit");
    let plot = normal_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::normal_probability(&plot, &dist, "test");
    assert_eq!(
        labels(&spec),
        [
            "1", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95", "99"
        ]
    );
}

#[test]
fn normal_n200_uses_the_captured_0_1_to_99_9_set() {
    let sample = increasing(200, 10.0, 0.05);
    let dist = Normal::fit_moments(&sample).expect("fit");
    let plot = normal_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::normal_probability(&plot, &dist, "test");
    assert_eq!(
        labels(&spec),
        [
            "0.1", "1", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95", "99",
            "99.9"
        ]
    );
}

#[test]
fn weibull_n20_uses_the_captured_gumbel_set() {
    let sample = increasing(20, 1.0, 0.5);
    let dist = Weibull::fit_mle(&sample).expect("fit");
    let plot = weibull_probability_plot(&sample, &dist, PlottingPosition::Benard).expect("plot");
    let spec = PlotSpec::weibull_probability(&plot, &dist, "test");
    assert_eq!(
        labels(&spec),
        [
            "1", "2", "3", "5", "10", "20", "30", "40", "50", "60", "70", "80", "90", "95", "99"
        ]
    );
}
