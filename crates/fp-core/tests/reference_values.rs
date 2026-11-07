// SPDX-License-Identifier: MIT
//! Reference values: precision against published values and independent fits.
//!
//! `tests/reference/normal.json` and `tests/reference/distributions.json` hold
//! the committed values. The test does not run an external process: it compares
//! against that versioned JSON.

use fp_core::dists::{
    Distribution, Exponential, ExtremeValue, Gamma, Logistic, Loglogistic, Lognormal, Normal,
    Weibull,
};
use fp_core::special::{normal_cdf, normal_pdf, normal_quantile};
use serde_json::Value;

fn reference() -> Value {
    let raw = include_str!("reference/normal.json");
    serde_json::from_str(raw).expect("valid JSON reference")
}

fn distribution_reference() -> Value {
    let raw = include_str!("reference/distributions.json");
    serde_json::from_str(raw).expect("valid JSON distribution reference")
}

fn parameter(family: &Value, name: &str) -> f64 {
    family["parameters"][name]
        .as_f64()
        .unwrap_or_else(|| panic!("missing parameter {name}"))
}

/// Builds the engine distribution named by a reference entry.
fn construct(family: &Value) -> Box<dyn Distribution> {
    let p = |name: &str| parameter(family, name);
    match family["name"].as_str().expect("family name") {
        "Normal" => Box::new(Normal::new(p("mu"), p("sigma")).expect("normal")),
        "Lognormal" => Box::new(Lognormal::new(p("mu"), p("sigma")).expect("lognormal")),
        "Weibull" => Box::new(Weibull::new(p("shape"), p("scale")).expect("weibull")),
        "Exponential" => Box::new(Exponential::new(p("theta")).expect("exponential")),
        "Logistic" => Box::new(Logistic::new(p("mu"), p("sigma")).expect("logistic")),
        "Loglogistic" => Box::new(Loglogistic::new(p("scale"), p("shape")).expect("loglogistic")),
        "Gamma" => Box::new(Gamma::new(p("shape"), p("scale")).expect("gamma")),
        "ExtremeValue" => {
            Box::new(ExtremeValue::new(p("location"), p("scale")).expect("extreme value"))
        }
        other => panic!("unknown reference family {other}"),
    }
}

#[test]
fn cdf_matches_the_reference() {
    let r = reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for case in r["cdf"].as_array().expect("cdf list") {
        let z = case["z"].as_f64().expect("z");
        let expected = case["value"].as_f64().expect("value");
        let got = normal_cdf(z);
        assert!(
            (got - expected).abs() <= tol,
            "Φ({z}) = {got}, expected {expected} (tol {tol})"
        );
    }
}

#[test]
fn quantiles_match_the_reference() {
    let r = reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for case in r["quantile"].as_array().expect("quantile list") {
        let p = case["p"].as_f64().expect("p");
        let expected = case["value"].as_f64().expect("value");
        let got = normal_quantile(p);
        assert!(
            (got - expected).abs() <= tol,
            "Φ⁻¹({p}) = {got}, expected {expected} (tol {tol})"
        );
    }
}

#[test]
fn density_matches_the_reference() {
    let r = reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for case in r["pdf"].as_array().expect("pdf list") {
        let z = case["z"].as_f64().expect("z");
        let expected = case["value"].as_f64().expect("value");
        let got = normal_pdf(z);
        assert!(
            (got - expected).abs() <= tol,
            "φ({z}) = {got}, expected {expected} (tol {tol})"
        );
    }
}

#[test]
fn general_normal_composed_matches_the_standard() {
    // N(µ, σ) must be the transformed standard; checks the composition.
    let n = Normal::new(10.0, 2.0).expect("valid");
    for (p, z) in [
        (0.05, -1.644_853_626_951_472_2),
        (0.5, 0.0),
        (0.99, 2.326_347_874_040_841),
    ] {
        let got = n.quantile(p);
        let expected = 10.0 + 2.0 * z;
        assert!((got - expected).abs() < 1e-9, "p={p}: {got} vs {expected}");
    }
}

#[test]
fn distribution_cdfs_match_the_reference() {
    let r = distribution_reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for family in r["families"].as_array().expect("family list") {
        let name = family["name"].as_str().expect("family name");
        let distribution = construct(family);
        for case in family["cdf"].as_array().expect("cdf list") {
            let x = case["x"].as_f64().expect("x");
            let expected = case["value"].as_f64().expect("value");
            let got = distribution.cdf(x);
            assert!(
                (got - expected).abs() <= tol,
                "{name}: F({x}) = {got}, expected {expected} (tol {tol})"
            );
        }
    }
}

#[test]
fn distribution_densities_match_the_reference() {
    let r = distribution_reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for family in r["families"].as_array().expect("family list") {
        let name = family["name"].as_str().expect("family name");
        let distribution = construct(family);
        for case in family["pdf"].as_array().expect("pdf list") {
            let x = case["x"].as_f64().expect("x");
            let expected = case["value"].as_f64().expect("value");
            let got = distribution.pdf(x);
            assert!(
                (got - expected).abs() <= tol,
                "{name}: f({x}) = {got}, expected {expected} (tol {tol})"
            );
        }
    }
}

#[test]
fn distribution_quantiles_match_the_reference() {
    let r = distribution_reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    for family in r["families"].as_array().expect("family list") {
        let name = family["name"].as_str().expect("family name");
        let distribution = construct(family);
        for case in family["quantile"].as_array().expect("quantile list") {
            let p = case["p"].as_f64().expect("p");
            let expected = case["value"].as_f64().expect("value");
            let got = distribution.quantile(p);
            assert!(
                (got - expected).abs() <= tol * (1.0 + expected.abs()),
                "{name}: Q({p}) = {got}, expected {expected} (tol {tol})"
            );
        }
    }
}

#[test]
fn distributions_reject_parameters_outside_their_domain() {
    assert!(Normal::new(0.0, 0.0).is_err(), "zero scale");
    assert!(Lognormal::new(0.0, f64::NAN).is_err(), "NaN scale");
    assert!(Weibull::new(-1.0, 1.0).is_err(), "negative shape");
    assert!(Exponential::new(0.0).is_err(), "zero mean");
    assert!(
        Logistic::new(f64::INFINITY, 1.0).is_err(),
        "infinite location"
    );
    assert!(Loglogistic::new(0.0, 1.0).is_err(), "zero scale");
    assert!(Gamma::new(1.0, -2.0).is_err(), "negative scale");
    assert!(ExtremeValue::new(0.0, 0.0).is_err(), "zero scale");
}

fn fit_reference() -> Value {
    let raw = include_str!("reference/fits.json");
    serde_json::from_str(raw).expect("valid JSON fit reference")
}

/// Parameter names of each family, in `Distribution::parameters` order.
fn parameter_order(family: &str) -> &'static [&'static str] {
    match family {
        "Normal" | "Lognormal" | "Logistic" => &["mu", "sigma"],
        "Weibull" | "Gamma" => &["shape", "scale"],
        "Exponential" => &["theta"],
        "Loglogistic" => &["scale", "shape"],
        "ExtremeValue" => &["location", "scale"],
        other => panic!("unknown family {other}"),
    }
}

fn fit_engine(family: &str, sample: &fp_core::data::Sample) -> Box<dyn Distribution> {
    use fp_core::dists::{
        Exponential, ExtremeValue, Gamma, Logistic, Loglogistic, Lognormal, Normal, Weibull,
    };
    match family {
        "Normal" => Box::new(Normal::fit(sample).expect("normal fit")),
        "Lognormal" => Box::new(Lognormal::fit_mle(sample).expect("lognormal fit")),
        "Weibull" => Box::new(Weibull::fit_mle(sample).expect("weibull fit")),
        "Exponential" => Box::new(Exponential::fit_mle(sample).expect("exponential fit")),
        "Logistic" => Box::new(Logistic::fit_mle(sample).expect("logistic fit")),
        "Loglogistic" => Box::new(Loglogistic::fit_mle(sample).expect("loglogistic fit")),
        "Gamma" => Box::new(Gamma::fit_mle(sample).expect("gamma fit")),
        "ExtremeValue" => Box::new(ExtremeValue::fit_mle(sample).expect("extreme-value fit")),
        other => panic!("unknown family {other}"),
    }
}

fn reference_sample(dataset: &Value) -> fp_core::data::Sample {
    let values: Vec<f64> = dataset
        .as_array()
        .expect("dataset list")
        .iter()
        .map(|value| value.as_f64().expect("observation"))
        .collect();
    fp_core::data::Sample::new(values, fp_core::data::MissingPolicy::Drop).expect("sample")
}

#[test]
fn fits_match_the_reference_reference() {
    let r = fit_reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    let datasets = &r["datasets"];
    for entry in r["fits"].as_array().expect("fit list") {
        let family = entry["family"].as_str().expect("family");
        let sample = reference_sample(&datasets[entry["dataset"].as_str().expect("dataset")]);
        let distribution = fit_engine(family, &sample);
        let parameters = distribution.parameters();
        let expected_names = parameter_order(family);
        assert_eq!(parameters.len(), expected_names.len(), "{family}");
        for (index, name) in expected_names.iter().enumerate() {
            let expected = entry["parameters"][*name]
                .as_f64()
                .expect("expected parameter");
            let got = parameters[index].1;
            assert!(
                (got - expected).abs() <= tol * (1.0 + expected.abs()),
                "{family}/{}: {name} = {got}, reference {expected} (tol {tol})",
                entry["dataset"].as_str().unwrap_or("?")
            );
        }
    }
}

#[test]
fn fit_log_likelihoods_match_the_reference_reference() {
    let r = fit_reference();
    let tol = r["tolerance"].as_f64().expect("tolerance");
    let datasets = &r["datasets"];
    for entry in r["fits"].as_array().expect("fit list") {
        let family = entry["family"].as_str().expect("family");
        let sample = reference_sample(&datasets[entry["dataset"].as_str().expect("dataset")]);
        let distribution = fit_engine(family, &sample);
        let got: f64 = sample
            .values()
            .iter()
            .map(|&x| distribution.ln_pdf(x))
            .sum();
        let expected = entry["log_likelihood"].as_f64().expect("log-likelihood");
        assert!(
            (got - expected).abs() <= tol * (1.0 + expected.abs()),
            "{family}: log-likelihood {got}, reference {expected} (tol {tol})"
        );
    }
}

#[test]
fn fits_reject_degenerate_samples() {
    use fp_core::data::{MissingPolicy, Sample};
    let constant = Sample::new(vec![2.0; 8], MissingPolicy::Drop).expect("sample");
    assert!(Normal::fit(&constant).is_err(), "constant normal");
    assert!(Lognormal::fit_mle(&constant).is_err(), "constant lognormal");
    assert!(Weibull::fit_mle(&constant).is_err(), "constant weibull");
    assert!(Logistic::fit_mle(&constant).is_err(), "constant logistic");
    assert!(
        Loglogistic::fit_mle(&constant).is_err(),
        "constant loglogistic"
    );
    assert!(Gamma::fit_mle(&constant).is_err(), "constant gamma");
    assert!(
        ExtremeValue::fit_mle(&constant).is_err(),
        "constant extreme value"
    );

    let non_positive = Sample::new(vec![1.0, 0.0, 2.0], MissingPolicy::Drop).expect("sample");
    assert!(Weibull::fit_mle(&non_positive).is_err(), "zero observation");
    assert!(
        Lognormal::fit_mle(&non_positive).is_err(),
        "zero observation"
    );
    assert!(Gamma::fit_mle(&non_positive).is_err(), "zero observation");
    assert!(
        Loglogistic::fit_mle(&non_positive).is_err(),
        "zero observation"
    );
}
