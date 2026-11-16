# v1.1 test plan against the reference

Status: planning document for the v1.1 validation round.
Date: 2026-11-16.
Scope: the ten P2 items delivered in this cycle. It defines what is executed,
which published worked example each case reads, and the acceptance criterion.
It is a test plan only: no code, fixture or version changes are part of it.

The reference is the published help-and-how-to documentation of the commercial
package the project uses as a behavioural benchmark. This document describes
topics and data shapes, never its name, its files or its screenshots. Every
reference value that becomes a committed fixture is entered in the reference
corpus with a `source` and a positive `tolerance`, as the corpus rules require.

## 1. Setup

- **Builds under test**: the Windows x86_64 and Linux aarch64 artifacts of the
  latest green build, installed with the project fetch script. No local build
  and no local native test run is part of the plan.
- **Reference environment**: the reference package at its current release, with
  the help topics listed below open at the worked examples.
- **Worksheets**: one CSV per case, exported from the case data. The CSV files
  live in the private validation notes while a case is open; only the values
  that a committed test reads are transcribed into the corpus, with the
  derivation note.
- **Recordings**: session text, chart export and, for dialogs, a screenshot.
  They stay private; the plan and the corpus never carry screenshots.
- **Execution order**: engine and report values first, then the dialog and the
  help page, then the CLI on the same CSV. A case passes only when all three
  agree with each other and with the recorded reference value.

## 2. Tolerance classes

| Class | Meaning | Rule |
|---|---|---|
| **A** | Exact | Codes, counts, orders, decisions, sample sizes, row/column structure, refusal messages, panel and series counts. Zero difference. |
| **B** | Closed-form algebra | Relative difference ≤ `1e-9`. |
| **C** | Likelihood, ANOVA and regression tables | Relative difference ≤ `1e-6`; probabilities compared as probabilities, not as rounded text. |
| **D** | Iterative nonlinear fits | Relative difference ≤ `1e-4` on estimates and standard errors; convergence state must agree. |
| **E** | Monte Carlo | `|p − p_ref| ≤ 3·√(p(1−p)/B) + 1/B` with the declared `B`; a ranking of effects or importance must agree exactly. |
| **F** | Charts | Same panels, titles, axis labels and drawn element counts; a visual check of the shape. |

A documented approximation (for example a conservative critical value) is
accepted only when the plan says so and the report and the help page state the
approximation. Any other difference is a defect.

## 3. P2 items

### 3.1 General factorial design, factorial analysis and screening

Engine: `doe.rs`, `factorial.rs`. Reference topics: *Create Factorial Design*,
*Analyze Factorial Design* and *Analyze Screening Design*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | 2³ full factorial, two replicates, coded levels | *Create Factorial Design* worked example | `StdOrder`, `RunOrder`, coded columns and replicate count | A |
| 2 | Analysis of the two-level design with replicates | *Analyze Factorial Design* worked example | Term list and order; coefficients, sequential and adjusted sums of squares, F and p | A / C |
| 3 | Screening of a 12-run two-level design with one large effect | *Analyze Screening Design* worked example | Effect of every factor, Lenth pseudo standard error, individual and simultaneous margins, significance decision | C / A |
| 4 | Saturated design without error | Documented behaviour of the topic | Pseudo standard error handling and the warning; no silent zero | A |
| 5 | Text factor with a missing row | Topic data-shape notes | Factor level set, dropped row, marginal means | A / C |
| 6 | Invalid design: one level, duplicate factor, mismatched level list | Documented refusals | Error, no partial worksheet | A |

### 3.2 Analysis of means (ANOM)

Engine: `anom.rs`. Reference topic: *Analysis of Means*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Balanced design, five levels of five observations | *Analysis of Means* worked example | Weighted grand mean, decision limits, per-level decisions | C / A |
| 2 | Unequal group sizes | The unbalanced example of the topic | Per-level limits and decisions | C / A |
| 3 | One level clearly outside the limits | Same data, one shifted group | Decision table and chart elements | A / F |
| 4 | Critical value | Published studentized maximum modulus table | Record the difference of the conservative Bonferroni-t approximation; decisions must agree | A on decisions |
| 5 | Constant sample, single level, two observations total | Documented refusals | Error, no report | A |

### 3.3 Time-series diagnostics

Engine: `tsdiag.rs`. Reference topics: *Autocorrelation*, *Cross Correlation*,
*Augmented Dickey-Fuller Test*, *Differences* and *Trend Analysis*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | ACF, PACF and Ljung–Box of a trended series | *Autocorrelation* worked example | Correlations at every lag, Q statistic, degrees of freedom, p | C |
| 2 | Cross-correlation of two shifted series | *Cross Correlation* worked example | Correlations at lags `−m…m`, pairs per lag, largest absolute value and its lag | C / A |
| 3 | Unit root with constant and with constant plus trend | *Augmented Dickey-Fuller* worked example | τ statistic, critical values, decision at 5 % | C / A |
| 4 | ADF p-value | The topic's p-value table | Difference from the interpolated approximation, recorded; decision unchanged | C band, A decision |
| 5 | Second differences and a one-period lag | *Differences* worked example | Stored column values and missing rows | A |
| 6 | Linear, quadratic, exponential and growth trends | *Trend Analysis* worked example | Coefficients, R², fitted values | C / B |
| 7 | Invalid window, lag and order; non-positive series for the growth model | Documented refusals | Error, no partial output | A |

### 3.4 2×2 crossover equivalence

Engine: `equivalence.rs`. Reference topic: the 2×2 crossover equivalence
worked example (long table with subject, sequence, period and treatment).

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Balanced crossover, twelve subjects | The topic's balanced example | Within-subject differences, unweighted treatment estimate, standard error, degrees of freedom, 90 % interval, TOST p and decision | C / A |
| 2 | Unequal sequence sizes | The topic's unbalanced example | Unweighted estimate, standard error, interval | C |
| 3 | Analysis of variance | Same data | Sequence/carryover, subject, period, treatment and residual rows; sums of squares, F and p | A / C |
| 4 | Asymmetric limits | Documented limits option | Interval and both one-sided p-values | C / A |
| 5 | Broken designs: missing period, repeated treatment, equal sequence orders, crossed limits | Documented refusals | Error, no report | A |

### 3.5 MANOVA and nested ANOVA

Engine: `manova.rs`, `nested.rs`. Reference topics: *MANOVA* and *Nested ANOVA*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | MANOVA, three responses and three groups | *MANOVA* worked example | H and E matrices, eigenvalues, Wilks, Pillai, Hotelling–Lawley and Roy, Rao F with both degrees of freedom and p, group means | C |
| 2 | Two groups (exact F) | The topic's two-group case | The four statistics and the exact F | C |
| 3 | Nested design 2 groups × 3 subgroups × 2 observations | *Nested ANOVA* worked example | Sums of squares, hierarchical F, variance components and percentages | C |
| 4 | Zero or negative variance component | The topic's note on negative estimates | Zero component and warning | A |
| 5 | Unbalanced nested design, singular error matrix | Documented refusals | Error naming the reason | A |

### 3.6 Nonlinear regression

Engine: `nonlinear.rs`. Reference topic: *Nonlinear Regression*, including its
enzyme-kinetics example and the growth-model examples.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Michaelis–Menten fit | Enzyme-kinetics worked example | Estimates, asymptotic standard errors, t, p, parameter intervals, R², residual standard error, iterations and convergence | D / C |
| 2 | Asymptotic, logistic, Gompertz, Weibull growth and power models | The topic's six-model examples | Estimates and standard errors; convergence | D |
| 3 | Explicit starting values | The topic's starting-value example | Same optimum as case 1; no dependence on the automatic start | D |
| 4 | Fitted curve | Case 1 | Values at twenty evenly spaced predictor points | D |
| 5 | Invalid domain (`x ≤ 0` for the power and Weibull models), duplicate response/predictor, short sample | Documented refusals | Error, no partial fit | A |

### 3.7 Survival and reliability

Engine: `survival.rs`, `reliability.rs`. Reference topics: *Parametric
Distribution Analysis (Right Censoring)* and the survival plot of the fitted
model.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Right-censored time and censor columns | The topic's censored worked example | Exponential, Weibull and lognormal maximum-likelihood parameters, log-likelihood, AIC and BIC | C |
| 2 | Model ordering | Case 1 | Order by AIC and the reported criterion values | A / C |
| 3 | Percentile lives | Case 1 | B1, B5, B10 and B50 of every model | C |
| 4 | Mission time | Case 1 | Reliability and hazard of every model at the chosen time | C |
| 5 | Kaplan–Meier overlay | Case 1 | Curve points and censoring marks | C / A |
| 6 | No censor column (all failures) | The topic's complete-data example | Parameters and criteria | C |
| 7 | Invalid mission time, non-positive time, text column | Documented refusals | Error, no report | A |

### 3.8 CART and random forests

Engine: `tree.rs`. Reference topics: *CART Regression*, *CART
Classification* and *Random Forests*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Regression tree, numeric response and four predictors | *CART Regression* worked example | Split variables and thresholds, leaves, depth, R² and RMSE | A / C |
| 2 | Classification tree, binary response | *CART Classification* worked example | Splits, confusion matrix, accuracy, class order | A / C |
| 3 | Random forest, one hundred trees | *Random Forests* worked example | Out-of-bag error inside the Monte Carlo band; importance ranking; reproducibility with the same seed | E / A |
| 4 | Ties and alternative splits | Case 1 | First divergent split recorded; same R² within `1e-3` and the same importance ranking required | A / C |
| 5 | Pre-pruning by depth, minimum leaf and minimum gain | Documented options | Tree structure at each limit | A |
| 6 | Invalid predictor list, constant response, single class, bad options | Documented refusals | Error, no tree | A |

### 3.9 Linear-model power and sample size

Engine: `power.rs`. Reference topics: *Power and Sample Size for Multiple
Regression* and the general linear model power example.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Overall regression test, three predictors, R² 0.25, α 5 %, power 80 % | *Multiple Regression* worked example | Smallest sample size that reaches the power; power at that size | A / C |
| 2 | Partial F test, three of five terms, f² 0.15 | The topic's increase-in-R² example | Sample size, power, numerator and denominator degrees of freedom, non-centrality | A / C |
| 3 | Single coefficient (`q = 1`) | The topic's one-term case | Agreement with the corresponding t test and with the reference | A / C |
| 4 | α at 1 % and 10 %, power at 90 % | The topic's option table | Sample size and power | A / C |
| 5 | Invalid tested terms, model terms, α and power | Documented refusals | Error, no report | A |

### 3.10 Data calculations and resampling

Engine: `calcdata.rs`. Reference topics: *Standardize*, *Patterned Data*,
*Mesh Data*, *Make Indicator Variables*, *Column Statistics*, *Row Statistics*,
*Transpose*, *Matrix Multiply*, *Random Data*, *Bootstrap* and *Randomization
Test*.

| # | Case | Reference example | Compare | Class |
|---|---|---|---|---|
| 1 | Standardize a column | *Standardize* worked example | Every z value, mean zero and standard deviation one, missing cells | C |
| 2 | Patterned sequence with repeats | *Patterned Data* worked example | Values, order, row count | A |
| 3 | Mesh of two and of three factors | *Mesh Data* worked example | Grid order, level values, row count | A |
| 4 | Indicator variables, three levels, reference dropped | *Make Indicator Variables* worked example | Column names, 0/1 values, order | A |
| 5 | Column statistics and row statistics | The two worked examples | N, N*, sum, mean, standard error, standard deviation, quartiles and extremes; per-row values with missing cells | C |
| 6 | Transpose and matrix product | *Transpose* and *Matrix Multiply* worked examples | Values and dimensions | A |
| 7 | Random data, Normal, Uniform and Exponential | *Random Data* worked example | Bounds and distribution moments inside the Monte Carlo band; reproducibility inside FreePlots with the same seed | E / A |
| 8 | Bootstrap of five rows and three resamples | *Bootstrap* worked example | Sample column, row count and structure; values from the source rows | A |
| 9 | Randomization test of two separated samples, 999 permutations | *Randomization Test* worked example | Observed difference, p-value inside the Monte Carlo band, decision and chart | E / A |
| 10 | Invalid selection, empty name, caps exceeded | Documented refusals | Error, no sheet or report | A |

## 4. Cross-cutting checks (every item)

1. The dialog opens from its menu path, runs the same worksheet through the
   same options as the engine case and shows the same values.
2. The help page exists in English and in Portuguese, names the data shape and
   the example, and closes with Escape.
3. The CLI command on the same CSV produces the same session values as the
   dialog (class A for text structure, class C for numbers).
4. The session text names the method and the statistics, and the chart has the
   panels and series the help page describes.
5. No fixture, CSV or document names another product or carries a screenshot.

## 5. Exit criteria

- Every case above executed on both artifacts; one result record per case with
  the reference value, the FreePlots value, the class and the outcome.
- All class-A checks pass; no class-B, C or D difference exceeds its tolerance;
  every class-E difference is inside the band.
- Every accepted divergence has a written cause (documented approximation,
  iteration path, tie-breaking, Monte Carlo seed) and is recorded in the
  reference corpus when it becomes a fixture.
- Failures become defects with the item number and the case number; the round
  closes only when the summary table has no open failure.
- The summary table reports, per item: cases executed, passed, accepted
  divergence, failed.

## 6. Traceability

| Item | Engine | Dossier | Public commit |
|---|---|---|---|
| P2-33 general factorial and screening | `doe.rs`, `factorial.rs` | `decisions-p2-factorial.md` | `4e994e0` |
| P2-24 analysis of means | `anom.rs` | `decisions-p2-anom.md` | `195448d` |
| P2-29 time-series diagnostics | `tsdiag.rs` | `decisions-p2-timeseries.md` | `89362a8` |
| P2-38 2×2 crossover equivalence | `equivalence.rs` | `decisions-p2-equivalence.md` | `d953f6d` |
| P2-23 MANOVA and nested ANOVA | `manova.rs`, `nested.rs` | `decisions-p2-manova-nested.md` | `f3cb44e` |
| P2-25 nonlinear regression | `nonlinear.rs` | `decisions-p2-nonlinear.md` | `f3b0079` |
| P2-36 survival and reliability | `survival.rs`, `reliability.rs` | `decisions-p2-reliability.md` | `21bfff8` |
| P2-37 CART and random forests | `tree.rs` | `decisions-p2-trees.md` | `9ef23c9` |
| P2-39 linear-model power | `power.rs` | `decisions-p2-power.md` | `d697a8b` |
| P2-40/41 calculations and resampling | `calcdata.rs` | `decisions-p2-calc-bootstrap.md` | `db64f66` |
