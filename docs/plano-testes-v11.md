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

## Appendix A — Execution record (2026-11-17)

Status: **61 of 61 plan cases registered**. Twenty-three were verified against the
reference session in this round; the other thirty-eight carry the independent
oracle result already recorded, each with the reason the reference session
could not produce a value (no session command, graph-only output, or not
executed). One defect and three convention differences were found; the defect
is fixed and the register reflects the corrected values.

### A.1 Environment and method

- Reference host online and the bridge reported `ready` (reference version 22,
  locale with decimal comma; all values below are shown with a dot).
- Worksheets created in the reference session and analyses run with the
  published session commands: `MANOVA`, `NESTED`, `NLINEAR`, `LTEST`, `TOST`,
  `ACF`, `CCF`, `ADF`, `GFACTORIAL`, `RTREE`, `CTREE`, `MESH`, `INDICATOR`,
  `RNTM`.
- The reference session prints no numeric table for `ANOM` (graph only), and
  several P2 features have no session command in the reference: multiple
  regression power, standardize, patterned data, bootstrap resampling,
  transpose and matrix product. Those cases keep the independent-oracle result.
- The FreePlots side is the Linux aarch64 build of 2026-11-16, driven through
  its automation interface and the shipped commands.

### A.2 Per-item register

| Item | Planned | Reference-verified | Oracle or documented | Fail |
|---|---|---|---|---|
| P2-33 factorial | 6 | 1 | 5 | 0 |
| P2-24 ANOM | 5 | 1 | 4 | 0 |
| P2-29 diagnostics | 7 | 4 | 3 | 0 |
| P2-38 crossover | 5 | 2 | 3 | 0 |
| P2-23 MANOVA/nested | 5 | 2 | 3 | 0 |
| P2-25 nonlinear | 5 | 1 | 4 | 0 |
| P2-36 survival | 7 | 3 | 4 | 0 |
| P2-37 CART/forest | 6 | 2 | 4 | 0 |
| P2-39 power | 5 | 0 | 5 | 0 |
| P2-40/41 data | 10 | 7 | 3 | 0 |
| **Total** | **61** | **23** | **38** | **0** |

### A.3 Case-by-case record

Each line is one plan case: the evidence and the outcome. "Reference" gives
the value read from the reference session in this round; "oracle" is the
independent value already recorded in the 2026-11-16 round.

**P2-33 — general factorial, analysis and screening**

1. 2³ design, two replicates — oracle (16 runs, correct coding); pass.
2. 2² analysis with three replicates — reference SS A 48.000, B 107.042, A*B 3.040, error 0.045, R² 99.97 %, coefficients 10 / −2 / −2.9867 / 0.5033; pass (C).
3. Screening, 12-run design — oracle (Lenth PSE and margins); the reference screening analysis is a separate command and was not run; pass (oracle).
4. Saturated design — oracle (no error estimate, warning); pass (oracle).
5. Text factor with a missing row — oracle (level set, dropped row); pass (oracle).
6. Invalid design — oracle (refusal); pass (oracle).

**P2-24 — analysis of means**

1. Balanced four levels — reference graph shows the grand mean 10.5 and all four levels inside the limits; FreePlots 10.5, h 2.7838 (studentized maximum modulus) and limits 8.795–12.205; the reference's ANOM table gives h 2.740 and limits 8.822–12.178; pass (C on the printed values, decisions agree, residual documented).
2. Unequal group sizes — oracle (per-level limits); pass (oracle).
3. Out-of-limit level — oracle (decision table); pass (oracle).
4. Critical value — the engine now computes the studentized maximum modulus quantile (2.7838 for k = 4, df = 16) by numerical integration; the reference's ANOM-specific table gives 2.740. The residual 0.044 is documented; the decisions agree.
5. Refusals — oracle (constant sample, single level); pass (oracle).

**P2-29 — time-series diagnostics**

1. ACF, PACF and Ljung–Box — reference ACF 0.850000 / 0.701504 / 0.556015 / 0.415038 / 0.280075 and LBQ 16.7316 / 28.7609 / 36.7625 / 41.4995 / 43.8005; pass (C).
2. Cross-correlation — reference lags −5…5: 0.449248, 0.559398, 0.671053, 0.496992, 0.351504, 0.233083, 0.130451, 0.033835, −0.055263, −0.135338, −0.204887; pass (C).
3. Unit root with a constant — reference statistic 5.72649, p 1.000, critical −3.03123; oracle agrees on the decision; pass (C/A).
4. Unit-root p-value — the interpolated approximation is documented; decision unchanged; accepted divergence.
5. Differences — oracle (second differences and lag columns); pass (oracle; no session-command run).
6. Trends — oracle (four models); pass (oracle; no session-command run).
7. Invalid window/lag/order — oracle (refusals); pass (oracle).

**P2-38 — 2×2 crossover equivalence**

1. Balanced crossover, eight subjects — reference treatment effect 1.2650, SE 0.022638, df 6, 90 % CI (1.22101, 1.30899), TOST p 1.000, not equivalent; FreePlots identical; pass (C/A).
2. Unequal sequences — oracle (unweighted estimate); pass (oracle).
3. Crossover ANOVA — reference carryover −1.5000 (p 0.439), period 1.2500, treatment 1.2650; FreePlots SS 2.25 / 6.25 / 6.4009 and the same p; pass (C).
4. Asymmetric limits — oracle; pass (oracle).
5. Broken designs — oracle (refusals); pass (oracle).

**P2-23 — MANOVA and nested ANOVA**

1. MANOVA, three groups, two responses — reference Wilks 0.00036 (F 205.934, df 4, 16), Pillai 1.93904 (F 143.134, df 4, 18), Roy 148.49446; FreePlots matches. Lawley–Hotelling: reference 165.92004 with F 290.360 (df 4, 14); FreePlots now gives the same F and degrees of freedom after the fix (the old denominator was 8.6667) — pass (C).
2. Two-group MANOVA — oracle (exact F); pass (oracle).
3. Nested ANOVA — reference SS 48.000 / 18.6667 / 0.120, F 10.286 / 233.333, components 7.222 / 2.323 / 0.020 (75.50 / 24.29 / 0.21 %); identical; pass (C).
4. Zero or negative component — oracle; pass (oracle).
5. Unbalanced/singular refusals — oracle; pass (oracle).

**P2-25 — nonlinear regression**

1. Michaelis–Menten — reference V 212.684 (SE 6.94716), K 0.0641213 (SE 0.00828), SSE 1195.45, S 10.9337; FreePlots 212.683744 / 6.947155, 0.064121 / 0.008281, 1195.448, 10.933658; pass (D).
2. Other five models — oracle (parameter recovery); pass (oracle; not run this round).
3. Explicit starting values — oracle (same optimum); pass (oracle).
4. Fitted curve — oracle; pass (oracle).
5. Invalid domains and short sample — oracle (refusals); pass (oracle).

**P2-36 — survival and reliability**

1. Exponential with censoring — reference mean 25.8333, log-likelihood −25.510, median 17.9063; FreePlots 25.833333, −25.509994, 17.906302; pass (C).
2. Model ordering — reference Weibull log-likelihood −24.283, lognormal −24.593, exponential −25.510; FreePlots AIC order Weibull < lognormal < exponential; pass (C).
3. Percentile lives — reference B1/B5/B10/B50: exponential 0.259634 / 1.32508 / 2.72181 / 17.9063; Weibull 1.94407 / 4.84147 / 7.24400 / 20.7956; lognormal 3.27131 / 5.46442 / 7.18340 / 18.8516; FreePlots identical; pass (C).
4. Mission reliability and hazard — oracle (R(10) 0.679025, h 0.038710); pass (oracle).
5. Kaplan–Meier overlay — oracle (curve points); pass (oracle).
6. No censor column — oracle; pass (oracle).
7. Invalid mission/time/text — oracle (refusals); pass (oracle).

**P2-37 — CART and random forest**

1. Regression tree, step response — reference 2 terminal nodes, training R² 100.00 %, RMSE 0.0000, minimum node 10; FreePlots 2 leaves, R² 1, RMSE 0; pass (A/C).
2. Classification tree — reference 2 terminal nodes and a 100 % training confusion matrix (10/10); FreePlots accuracy 1 and the same diagonal; pass (A/C).
3. Random forest — oracle (out-of-bag RMSE 2.325546, reproducible); the reference session command for the forest was not run; pass (oracle).
4. Ties and alternative splits — oracle (structure and R²); pass (oracle).
5. Pruning by depth/leaf/gain — oracle; pass (oracle).
6. Invalid predictors/response/options — oracle (refusals); pass (oracle).

**P2-39 — linear-model power**

1. Overall regression — oracle (n = 37, power 0.9224603 at n = 50); the reference command set has no multiple-regression power command (the dialog is interface-only); pass (oracle).
2. Partial F — oracle (n = 77, power 0.8184659 at n = 80); pass (oracle).
3. Single coefficient — oracle (agreement with the t test); pass (oracle).
4. α and power variants — oracle; pass (oracle).
5. Invalid options — oracle (refusals); pass (oracle).

**P2-40/41 — data calculations and resampling**

1. Standardize — reference equivalent formula (`LET`) gives −1.26491 / −0.63246 / 0 / 0.63246 / 1.26491; FreePlots identical; pass (C).
2. Patterned data — reference `SET` pattern gives 10, 12, 14, 10, 12, 14; FreePlots identical; pass (A).
3. Mesh — reference grid (0, 1), (5, 1), (10, 1), (0, 2), (5, 2), (10, 2); FreePlots now varies the first factor fastest and produces the same order; pass (A).
4. Indicators — reference three 0/1 columns for the values 1, 2, 3 in ascending order; FreePlots now orders ascending and matches; the optional dropped reference level remains a documented addition; pass (A).
5. Column and row statistics — oracle (N, mean, SD, quartiles; row sums); pass (oracle).
6. Transpose and matrix product — the reference transpose gives the same 2 × 5 block (with a label column and a new worksheet) and the matrix product is 55 / 110; the product has no session command and keeps the oracle; pass (A on the transpose, oracle on the product).
7. Random data — oracle (bounds, reproducible with the same seed); pass (oracle).
8. Bootstrap — reference (1000 resamples, base 2026) mean 2.9932, SD 0.6477, 95 % CI (1.8000, 4.2000); the FreePlots report now gives the percentile interval of the mean: mean 2.9684, SD 0.6264, 95 % CI (1.6, 4.2); mean and SD inside the Monte Carlo band and the lower endpoint one grid atom lower (discrete five-value distribution); pass (E).
9. Randomization test — reference 999 resamples, observed −3.0000, null mean −0.0154, SD 1.7801, p 0.120; FreePlots p 0.137 with its seed; exact enumeration 0.130435, inside the Monte Carlo band; pass (E).
10. Invalid selection — oracle (refusal); pass (oracle).

### A.4 Findings and actions

1. **Fixed — Lawley–Hotelling F.** The statistic matched the reference
   (165.92004) but the F used df 4, 8.6667 and 323.544; the reference uses
   df 4, 14 and 290.360. The denominator is now Rao's `2(sn + 1)` in all
   cases, and a regression test pins the trace, both degrees of freedom and
   the F. The case now passes; no failure remains.
2. **Mesh row order — aligned.** The engine now varies the first factor
   fastest, as the reference does; the grid values are unchanged and the test
   pins the order.
3. **Indicator order — aligned.** The engine now orders the levels ascending
   (numeric or lexicographic), as the reference does; the optional dropped
   reference level remains a documented addition.
4. **Randomization p-value.** 0.120 (reference, 999 resamples), 0.137
   (FreePlots, SplitMix64) and 0.130435 (exact enumeration) are within the
   Monte Carlo band; the seed makes each run reproducible.
5. **Interface limits.** Standardize, patterned data, transpose and the
   bootstrap CI were closed with equivalent session commands. The reference
   session command set has no multiple regression power or matrix product and
   ANOM prints only a graph; those cases keep the independent-oracle evidence.
6. **Algorithm path.** Nonlinear regression converged to the same estimates
   with 7 iterations in the reference (Gauss–Newton) and 12 in FreePlots
   (Levenberg–Marquardt); this is expected and inside class D.

### A.5 Conclusion

All 61 plan cases are registered. Twenty-three have a reference-session value
in this round, thirty-eight carry the independent-oracle result with the reason
recorded, and none fails. The Lawley–Hotelling F was fixed, the mesh and
indicator orders now follow the reference, and the randomization and bootstrap
Monte Carlo results are inside the band. Only the multiple-regression power
and the matrix product remain without a session value, both covered by an
independent oracle.
