# Validation coverage

This page summarises the independent checks behind FreePlots. Every row is
reproducible from the public repository; no proprietary software or external
service is required. See `PROVENANCE.md` for the method and the oracle versions.

## Independent oracles

| Area | Oracle / reference | Status |
|---|---|---|
| Probability distributions and fits | SciPy 1.18.1; published likelihood formulas | Covered by `tools/gen_distributions.py`, `gen_fits.py` |
| Descriptive statistics, quartiles, Boxplot | NumPy 2.5.4; R-7 definition | Covered by `tools/gen_charts.py` |
| t tests, proportions, variances, equivalence | SciPy 1.18.1; closed-form formulas | Covered by `tools/gen_tests.py`, `gen_inference.py` |
| Non-parametric tests | SciPy 1.18.1 | Covered by `tools/gen_nonparametric.py` |
| One-way and two-way ANOVA, Tukey/Fisher | SciPy 1.18.1; hand-built sums of squares | Covered by `tools/gen_anova.py` |
| Regression and stepwise selection | statsmodels 0.15.0; hand-built OLS | Covered by `tools/gen_regression.py`, `gen_stepwise.py` |
| Logistic, ordinal and Poisson regression | statsmodels 0.15.0 | Covered by `tools/gen_ordinal.py`, `gen_regression.py` |
| PCA and k-means | NumPy 2.5.4; scikit-learn 1.9.1 | Covered by `tools/gen_pca.py`, `gen_kmeans.py` |
| Reliability and accelerated life | SciPy 1.18.1; lifelines 0.30.3 | Covered by `tools/gen_reliability.py`, `gen_alt.py`, `gen_kaplan_meier.py` |
| Time series and decomposition | statsmodels 0.15.0 | Covered by `tools/gen_timeseries.py` |
| DOE, RSM, Taguchi, Plackett–Burman | hand-built OLS; orthogonal-array definitions | Covered by `tools/gen_doe.py`, `gen_rsm.py` |
| Capability and control charts | published constants and closed-form limits; NumPy for arithmetic | Covered by `tools/gen_control*.py`, `gen_subgroups.py`, `gen_laney.py` |
| MSA (Gage R&R, linearity, agreement, Type 1) | hand-built ANOVA/t tests and kappa | Covered by `tools/gen_gage.py`, `gen_msa.py`, `gen_agreement.py` |
| Plot structure, axes and panels | deterministic SVG/golden checks | Covered by `crates/fp-core/tests/golden.rs` |
| Licence inventory and SBOM | `Cargo.lock` plus crate metadata | Covered by `scripts/licence-inventory.py --check` |

## Regression fixtures

The `validation/freplots/` files are FreePlots reports frozen by this project.
They are used to detect unintended changes in wording, tables and plot
structure. They are not another program's output and do not establish
equivalence with one.

## Open coverage

| Area | Why it remains open |
|---|---|
| Some graph-only conventions (exact tick sets, axis padding, band shape) | They are design choices checked against the documented rules and golden plots, not against a proprietary implementation |
| Analyses without an independent oracle yet | Listed in `docs/parity.md` with their current status; they are covered by closed-form tests and input validation, not by an oracle |
| Very large data sets | Performance is measured separately (1M rows in the capability and control-chart paths); it is not a numerical-accuracy claim |

## Interpretation

A row is marked covered only when the numerical test or the documented plot
check exists and runs in CI. Matching a formula or a common convention is not,
by itself, a claim of equivalence with another program. The absence of a check
is an explicit coverage gap, not evidence of a defect.
