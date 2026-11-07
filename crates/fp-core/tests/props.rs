// SPDX-License-Identifier: MIT
//! Property tests: invariants that must hold for *all* valid inputs, not just
//! for the hand-picked examples.

use fp_core::data::{MissingPolicy, Sample};
use fp_core::dists::{Distribution, Normal};
use fp_core::plot::{PlotSpec, PlottingPosition, normal_probability_plot, positions};
use proptest::prelude::*;

proptest! {
    #[test]
    fn plotting_positions_stay_strictly_between_0_and_1(n in 1usize..2000) {
        for method in [
            PlottingPosition::Benard,
            PlottingPosition::Blom,
            PlottingPosition::Hazen,
            PlottingPosition::Weibull,
            PlottingPosition::Cunnane,
        ] {
            let p = positions(n, method).expect("n >= 1");
            prop_assert_eq!(p.len(), n);
            prop_assert!(p.iter().all(|v| *v > 0.0 && *v < 1.0));
            prop_assert!(p.windows(2).all(|w| w[0] < w[1]));
        }
    }

    #[test]
    fn cdf_is_monotone(a in -40.0f64..40.0, b in -40.0f64..40.0) {
        let n = Normal::new(0.0, 1.0).expect("valid parameters");
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(n.cdf(lo) <= n.cdf(hi) + 1e-16);
        prop_assert!((0.0..=1.0).contains(&n.cdf(a)));
    }

    #[test]
    fn quantile_undoes_the_cdf(p in 1e-8f64..0.999_999_99) {
        let n = Normal::new(-3.0, 2.5).expect("valid parameters");
        let back = n.cdf(n.quantile(p));
        prop_assert!((back - p).abs() < 1e-10, "p={} back={}", p, back);
    }

    #[test]
    fn mean_lies_between_minimum_and_maximum(vals in prop::collection::vec(-1e6f64..1e6f64, 1..64)) {
        let s = Sample::new(vals, MissingPolicy::Drop).expect("n >= 1");
        let mean = s.mean().expect("n >= 1");
        prop_assert!(mean >= s.min().expect("n >= 1") - 1e-9);
        prop_assert!(mean <= s.max().expect("n >= 1") + 1e-9);
    }

    #[test]
    fn sorted_sample_is_a_permutation_of_the_original(vals in prop::collection::vec(-1e3f64..1e3f64, 1..64)) {
        let s = Sample::new(vals.clone(), MissingPolicy::Drop).expect("n >= 1");
        let mut sorted = s.sorted();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        let mut original = vals;
        original.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        prop_assert_eq!(sorted.len(), original.len());
        for (a, b) in sorted.iter().zip(original.iter()) {
            prop_assert!((a - b).abs() < 1e-12);
        }
    }

    #[test]
    fn normal_fit_matches_the_moments(vals in prop::collection::vec(-1e4f64..1e4f64, 3..64)) {
        let s = Sample::new(vals, MissingPolicy::Drop).expect("n >= 3");
        if let Ok(fit) = Normal::fit(&s) {
            prop_assert!((fit.mu() - s.mean().expect("n >= 1")).abs() < 1e-9);
            prop_assert!(fit.sigma() > 0.0);
        }
    }

    #[test]
    fn probability_ticks_are_ordered_and_inside_the_axis(vals in prop::collection::vec(-1e4f64..1e4f64, 3..200)) {
        let s = Sample::new(vals, MissingPolicy::Drop).expect("n >= 3");
        if let Ok(fit) = Normal::fit(&s) {
            let plot = normal_probability_plot(&s, &fit, PlottingPosition::Benard).expect("valid");
            let spec = PlotSpec::normal_probability(&plot, &fit, "p");
            let ordered = spec
                .y_axis
                .ticks
                .windows(2)
                .all(|w| w[0].position < w[1].position);
            let ticks_inside = spec.y_axis.ticks.iter().all(|t| {
                t.position >= spec.y_axis.min - 1e-12 && t.position <= spec.y_axis.max + 1e-12
            });
            let points_inside = spec.points.iter().all(|p| {
                p.y >= spec.y_axis.min - 1e-12 && p.y <= spec.y_axis.max + 1e-12
            });
            prop_assert!(ordered);
            prop_assert!(ticks_inside);
            prop_assert!(points_inside);
        }
    }
}
