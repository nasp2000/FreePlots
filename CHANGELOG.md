# Changelog

All notable changes to FreePlots are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- *Power and Sample Size…* has a *Linear Model (partial F test)* procedure:
  Cohen's `f²` of the terms under test, the tested terms `q` and the model
  terms `p`, with the non-central F of the partial test (`λ = f²·n`,
  `q` and `n − p − 1` degrees of freedom), the power or the smallest sample
  size, and the report of both degrees of freedom and the non-centrality.
  `q = p` reproduces the overall regression test and `q = 1` the t test of one
  coefficient. The dialog gained its own help page in both interface
  languages, and the CLI accepts `--terms` with `--test linear`.
- The control-chart dialog has a *How to choose subgroups…* page with an
  overview, the data shapes the chart accepts and a worked example of a label
  column. It is available in both interface languages.
- The method notes describe the X̄–R and X̄–S charts built from a label column:
  first-appearance order, dropped rows, 2–25 observations per subgroup and
  per-point limits.
- Normal capability accepts a *Subgroup column* (CLI `--subgroup-column`): the
  pooled within-subgroup standard deviation uses each label group's own mean
  and `n − g` degrees of freedom, so Cp/Cpk no longer require consecutive
  equal-size groups. It has its own *How to choose subgroups…* help page.
- Quality Tools has a *Capability Sixpack…* entry: six panels (histogram,
  normal probability plot, control chart, last 25 points and capability with
  Cp/Cpk/Pp/Ppk) over one normal capability study, with its own help page in
  both interface languages. It accepts the same subgroup size or label column
  and uses individuals, X̄–R or X̄–S as the subgroup size requires.
- Quality Tools has an *Attribute Capability…* entry: binomial capability
  (PPM defective) and Poisson capability (DPU and PPM defects), each with an
  exact confidence interval and the normal-equivalent process Z, over the same
  defect and size columns as the attribute charts. It has its own help page in
  both interface languages.
- Quality Tools has a *T Chart (Rare Events)…* entry: the time between
  successive events with the exponential or gamma model, limits at the
  0.135 %/99.865 % quantiles and the median as the centre line, with its own
  help page in both interface languages.
- Control Charts has a *CUSUM Chart…* entry that opens the control-chart
  dialog pre-set to the cumulative-sum chart. The CUSUM has its own help page
  (overview, data and example) in both interface languages, and the entry
  clears a leftover phase or subgroup column from a variable chart.
- Control Charts has an *Acceptance Chart…* entry: acceptance and rejection
  limits derived from LSL/USL, the accepted and rejectable proportions and the
  producer and consumer risks, with the decision of every subgroup mean and a
  known or estimated sigma. It has its own help page in both interface
  languages.
- Quality Tools has a *Johnson Transformation…* command: it fits Johnson SU,
  SB or SL (or the best of the three by AIC), writes the standard-normal
  scores as a new worksheet column and reports the parameters with the
  Anderson–Darling test before and after. It has its own help page in both
  interface languages.
- Regression has a *Predict…* command: it fits the model identified by the
  response and the numeric predictors and evaluates it at new points, with the
  confidence interval of the mean response and the prediction interval of a
  single observation. It has its own help page in both interface languages.
- Basic Statistics has a *Runs Test…* entry: the Wald–Wolfowitz test of
  randomness about an optional comparison value (the mean by default), with
  the observed and expected runs, Z and the two-sided p-value. It has its own
  help page in both interface languages.
- Basic Statistics has four more entries: *1-Sample Z…* (known standard
  deviation), *1-Sample Poisson Rate…*, *2-Sample Poisson Rate…* (exact or
  normal approximation) and *Poisson Goodness-of-Fit…* (chi-square with
  estimated or given mean), with a shared help page in both interface
  languages.
- Reliability has a *Survival Analysis…* entry: the exponential, Weibull and
  lognormal life models fitted by maximum likelihood to the same right-censored
  data and compared by the log-likelihood, AIC and BIC (ordered by AIC), with
  the percentile lives B1/B5/B10/B50, the reliability and hazard at a mission
  time and the fitted curves drawn over the Kaplan–Meier estimate. It has its
  own help page in both interface languages.
- Basic Statistics has a *2×2 Crossover Equivalence…* entry: the
  within-subject treatment difference of a two-period crossover with a TOST
  against configurable lower and upper limits, the 90 % interval of a 95 %
  decision, and the analysis of variance with the sequence (differential
  carryover), subject, period and treatment rows. It has its own help page in
  both interface languages.
- Basic Statistics has a *Graphical Summary…* entry (histogram, boxplot and
  the confidence intervals of the mean, median and standard deviation) and a
  *Store Descriptive Statistics…* command that writes the selected statistics
  as new worksheet columns, one row per variable. Both share a help page in
  both interface languages.
- Basic Statistics has a *Covariance…* matrix (pairwise complete rows, n−1
  divisor) and a *Tally…* command (counts, percent and cumulative columns for
  the unique values of any columns), with a shared help page in both interface
  languages.
- Graph has a *Probability Distribution Plot…* entry: a theoretical density or
  cumulative curve for one of the ten distributions, with its parameters and an
  optional shaded area that reports the probability. It needs no worksheet
  data and has its own help page in both interface languages.
- Time Series has an *ARIMA…* entry: AR(p), differencing d and MA(q) fitted
  by conditional sum of squares, with forecasts, ψ-based intervals, AIC/AICc/
  BIC and a best-order search over p, d, q from 0 to 2. It has its own help
  page in both interface languages.
- Time Series has five diagnostic entries sharing one dialog and one help page
  in both interface languages: *Autocorrelation…* (ACF, PACF and the
  Ljung–Box statistic with the correlograms), *Cross Correlation…*
  (correlations of two series at lags −m…m), *Unit Root Test…* (the augmented
  Dickey–Fuller test with the MacKinnon critical values and an interpolated
  p-value), *Differences…* (differences of order 1 or 2 or a lag, written as a
  new column) and *Trend Analysis…* (linear, quadratic, exponential and growth
  trends with the equation and the fitted curve).
- ANOVA has a *MANOVA…* entry: one factor and at least two numeric responses,
  with Wilks' lambda, Pillai's trace, the Hotelling–Lawley trace and Roy's
  greatest root, each with Rao's approximate F, and the group means. It has
  its own help page in both interface languages.
- ANOVA has a *Nested ANOVA…* entry: a balanced two-level design
  (subgroups inside groups) with the hierarchical F tests and the random-effects
  variance components (groups, subgroups, error and their percentages). It has
  its own help page in both interface languages.
- ANOVA has an *Analysis of Means…* entry: every level mean against
  simultaneous decision limits around the weighted grand mean, with unequal
  group sizes and an ANOM chart. The critical value is the conservative
  Bonferroni-t approximation of the studentized maximum modulus, stated in the
  report and the help page.
- DOE has a *General Factorial Design…* entry: a full factorial for two to six
  factors with two to ten levels each (coded from −1 to +1), replicates and
  randomised run order. Screening of many two-level factors remains the
  Plackett–Burman design; definitive screening designs are not implemented.
- DOE has an *Analyze Factorial Design…* entry with two modes. The general
  factorial fits every main effect and interaction of two to six factors with
  two to ten levels, with the sequential and adjusted sums of squares, the
  marginal means of every level and the residuals. The screening analysis
  reads a two-level design, reports the effect of every factor with Lenth's
  pseudo standard error and the individual and simultaneous margins, and
  draws the Pareto chart of the absolute effects and the normal plot of the
  effects. Both modes share one help page in both interface languages.
- DOE has an *Optimal Design…* entry: D-optimal or maximin-distance
  selection from the coded factorial candidates, augmentation of the current
  design and evaluation (D-efficiency, condition number and distances). It has
  its own help page in both interface languages.
- DOE has a *Mixture Design…* entry: simplex centroid, simplex lattice of a
  chosen degree and extreme vertices with optional per-component bounds,
  replicates and randomised run order. It writes `StdOrder`, `RunOrder` and
  one proportion column per component, and has its own help page in both
  interface languages.
- Regression has a *Partial Least Squares…* entry: PLS1 by NIPALS with one or
  more components, standardised coefficients, the cumulative R² per component,
  observed-versus-fitted and R² selection plots. It handles collinear
  predictors and has its own help page in both interface languages.
- Quality Tools has a *Variables Acceptance Sampling…* entry: a single-limit
  plan with a known sigma (`n` and the critical distance `k` from AQL/RQL and
  the two risks), the OC curve, comparison of `n:k` plans and the accept/reject
  decision for a sample. It has its own help page in both interface languages.
- Gage R&R has an expanded crossed report: optional lower and upper
  specification limits add %Tolerance (with the repeatability and
  reproducibility percentages) and the published 10 %/30 % acceptance bands
  classify %Gage R&R and %Tolerance as acceptable, marginal or unacceptable.
  The dialog has its own help page in both interface languages.
- ANOVA has a *General Linear Model…* entry: one numeric response from
  categorical factors (reference coding), numeric covariates and two-way
  interactions, with sequential and adjusted (Type III) sums of squares, F
  tests, the coefficient table and the residuals plot. It has its own help page
  in both interface languages.
- *Graph → More Graphs* adds nine descriptive charts: bar, line, area,
  heatmap, bubble, individual value, marginal, parallel coordinates and
  correlogram. They share one dialog and one help page in both interface
  languages; the heatmap and bubble plots use value classes because the engine
  fills with a single tone and draws fixed-radius points, and the pie chart and
  the 3D scatter are recorded as limitations.
- DOE has a *Factorial Plots…* entry: from a coded two-level design it draws
  one main-effects panel per factor, one interaction panel per pair and the
  cube of the corner means for two or three factors, with the marginal means
  in the report. It has its own help page in both interface languages.
- Regression has a *CART and Random Forest…* entry: one binary tree for a
  numeric response (sum-of-squares splits, leaves are means) or a categorical
  one (Gini splits, leaves are the majority class, confusion matrix), and a
  random forest of bootstrap trees with the out-of-bag error and the variable
  importance of both modes drawn as bars. The growth is pre-pruned by depth,
  minimum leaf and minimum gain, and the seed makes the run reproducible. It
  has its own help page in both interface languages.
- Regression has a *Nonlinear Regression…* entry: least squares by
  Levenberg–Marquardt for the Michaelis–Menten, asymptotic, three-parameter
  logistic, Gompertz, Weibull growth and power models, with automatic starting
  values, asymptotic standard errors, t, p and the confidence interval of every
  parameter, R², the iterations, the convergence and the fitted curve over the
  observations. It has its own help page in both interface languages.
- Regression has a *Best Subsets…* entry: it fits every non-empty subset of up
  to twelve numeric predictors, reports the two best models of each size by R²
  with their adjusted R², Mallows' Cp and s, and adds a table of the best
  subset by each criterion. A four-panel selection plot shows the four
  criteria against the number of predictors. It has its own help page in both
  interface languages.
- Quality Tools has an *Individual Distribution Identification…* entry: it
  fits fourteen distributions (Normal; Lognormal, Exponential, Weibull, Gamma
  and Loglogistic with 2 and 3 parameters; Logistic; Extreme Value and
  Smallest Extreme Value), ranks them with the Anderson–Darling test and
  AIC/BIC, and draws one probability panel per family. It can also store a
  Box-Cox and a Johnson column with the transformed values. It has its own help
  page in both interface languages.
- The Multivariate menu has a *Generalized Variance Chart…* entry and a
  *Multivariate EWMA Chart…* entry, sharing the multivariate dialog and one
  help page in both interface languages. The generalized variance chart plots
  the determinant of each subgroup covariance against the b1/b2 limits; the
  multivariate EWMA smooths the mean vector with λ and plots T² against the
  Phase I F limit. Both take a subgroup size and drop the rows with a missing
  value.

### Changed

- The tests for the label column cover interleaved labels, rows with a missing
  measurement or label, a chart family that does not accept the column and a
  singleton subgroup. Capability adds the pooled grouped estimate, the dropped
  rows, a singleton group, a single group and a non-normal model. The Sixpack
  covers the six titles and their contents, the I–MR/X̄–R/X̄–S choice, the
  label column, a short study and the models and sizes it refuses. The
  attribute capability covers the pooled binomial and Poisson estimates, the
  exact intervals, zero counts and the invalid counts, sizes and confidence.
  The T chart covers the exponential limits against the closed-form values,
  the gamma moments, zero and negative intervals, a single observation, a
  missing row and a point outside the lower limit. The CUSUM menu entry
  covers the pre-set chart, the cleared variable-chart columns and the help
  page. The Johnson command covers the written scores (mean 0, variance 1),
  an explicit family with a missing row, a duplicate name and a short sample.
  Prediction covers the fitted equation, the two nested intervals, the
  listwise excluded row and the invalid points and predictors. The runs test
  covers the published die sequence, the alternating sequence, the default
  comparison value, the one-sided and short samples and the removed missing
  row. The Z and Poisson tests cover the known sigma, the exact Poisson tails,
  the conditional two-sample test, the published goodness-of-fit example and
  the invalid inputs. The acceptance chart covers the published limits, the
  estimated sigma, the subgroups with a trailing row, the one-sided
  specifications and the invalid proportions and crossing limits. The
  multivariate charts cover the b1/b2 limits and a divergent subgroup, the zero
  determinant and the singular covariance, the MEWMA statistic, the invalid λ
  and the panels without degrees of freedom. The individual identification
  covers the fourteen families and their panels, the two stored transformation
  columns, the missing-value names, the non-positive data and the families
  skipped on them. The new distributions are tested against synthetic samples
  of their own family and against constant or one-point samples. Best subsets
  covers the informative and the noise predictor, the adjusted R² and Cp
  choice, the full-model Cp, the one-predictor and collinear cases and the
  invalid response, duplicate, over-limit and short samples. The factorial
  plots cover the two-factor panels and cube, a single factor, a three-factor
  cube with eight corners and the uncoded or empty factor lists. The expanded
  Gage R&R covers %Tolerance against the closed-form value, the acceptance
  verdict table, the no-limits case and the inverted limits. ARIMA covers the
  AR coefficient of a synthetic series, a differenced random walk, an MA term,
  the best-order table, the forecast table and the invalid orders, horizon,
  confidence, short and constant series. Variables sampling covers the
  published `n`/`k` values, the accepted and rejected samples, the lower limit,
  the comparison table and the invalid proportions/sigma. PLS covers the
  coefficient recovery, the collinear predictor with one component, the two
  panels and the empty, duplicated, response-as-predictor, too-many-component
  and constant cases. Mixture designs cover the seven-run centroid, the
  six-run lattice, the bounded extreme vertices, bounds that filter the
  centroid, replicate/randomisation reproducibility and the invalid component,
  degree and bound cases. Optimal designs cover the full-rank D design, the
  maximin corners, augmentation that keeps the existing runs and the invalid
  run count, duplicate factor, singular and uncoded cases. The general
  factorial covers the six mixed-level runs, the full level coding, replicates
  and the mismatched level list. The factorial analysis covers the recovered
  terms and marginal means of a mixed-level design, the saturated design
  without error, a text factor with a missing row, the Lenth margins of a
  twelve-run screening design with one large effect, an undefined pseudo
  standard error and the uncoded, single-level and duplicate cases. The
  time-series diagnostics cover the closed-form ACF, PACF and Ljung–Box values
  of a ramp, the cross-correlation of a shifted series, a mean-reverting series
  and a random walk in the unit-root test (with the regression module as the
  cross-check), the difference and lag columns with a missing value, the four
  trend models and the invalid windows, lags, orders and non-positive values.
  The 2×2 crossover covers the recovered treatment and period effects with an
  equal-weight estimate, unequal sequence sizes with a carryover, the
  treatment F against the square of the t ratio, asymmetric limits and the
  broken designs (missing period, repeated treatment, equal sequence orders,
  crossed limits). The MANOVA covers the closed-form eigenvalues, matrices and
  statistics of a diagonal construction, the exact F with two groups and the
  invalid designs; the nested ANOVA covers the sums of squares, the hierarchical
  F tests, the variance components and percentages, a zero component and the
  unbalanced and constant cases. The nonlinear regression covers the published
  Michaelis–Menten fit of the enzyme-kinetics experiment (estimates, standard
  errors and R²), the recovery of the parameters of the other five models from
  noise-free curves, explicit starting values and the invalid domains,
  duplicate-column and short-sample cases. The survival comparison covers the
  closed-form exponential fit with censoring (total time over events, log
  likelihood, AIC and BIC), the percentile formulas of the three models, the
  mission reliability and hazard, the ordering by AIC and the invalid mission,
  column and text cases. The tree models cover a two-leaf regression tree on a
  step response, a perfectly separated classification with its confusion
  matrix, the out-of-bag error and the importance ranking of a forest with its
  reproducibility, and the invalid predictor, constant-response, single-class
  and option cases. The analysis of means covers the balanced and
  the unequal-size limits, an out-of-limits level, the missing-value count and
  the invalid alpha, single-level and constant-sample cases. The graph
  gallery covers the count/sum/mean bars, the line and area series, the
  heatmap classes, the bubble size classes, the individual values, the
  two-panel marginal, the parallel lines and the annotated correlogram, plus
  the invalid column lists. The general linear model covers the factor,
  covariate and interaction expansion, the Type I/III table, a model without
  interactions and the empty, duplicated, unknown-term and one-level cases.

## [0.1.1] - 2026-11-12

### Added

- The Help menu has a *Check for updates* item that opens the latest release
  page in the browser; the application itself still makes no network calls.
- The worksheet header has two lines per column: the automatic `C1` name stays
  on top and the user name (the one reports and graphs use) appears below.
  The lower line edits in place (double-click or type; Enter commits, Esc
  cancels), and a table pasted on it uses its first row as the column names
  and the remaining rows as data.

### Changed

- Release downloads are one `.zip` per platform: Windows x86_64 and
  Raspberry Pi 4 (aarch64). Each archive carries the executable, the licence
  inventory, the third-party notices, the SPDX SBOM and a short README, and
  `SHA256SUMS` covers the archives.
- Linux x86_64 is no longer published.
- The continuous integration workflow runs the 78 % line-coverage gate and
  the architecture guards on every push, so the layering rules and the
  coverage floor are enforced instead of being documented only. The guards
  live in `scripts/check-architecture.py` and skip test code.
- The million-row performance gate now covers descriptive statistics,
  capability (plain and Box–Cox), I–MR, X̄–R, regression (one and two
  predictors), logistic regression, ANOVA, two-way ANOVA, PCA, K-means, moving
  average, time-series decomposition and smoothing, scatterplot, the one- and
  two-sample and paired t tests, one-sample equivalence, histogram, boxplot,
  probability plot, ECDF, correlation and its matrix, partial correlation,
  normality, run charts, Kaplan–Meier, tolerance intervals, Mann–Whitney,
  Wilcoxon, Kruskal–Wallis, Friedman, Mood's median, the sign test, Levene,
  Bartlett, the chi-square test of independence, Grubbs, a curve fit, the
  interval plot, dotplot, matrix plot, series plot, Hotelling T², acceptance
  sampling and the exponential and Weibull reliability fits, plus a
  one-million-row CSV import.
- The censored Weibull fit computes the logarithms of the times once and
  scans eighty shapes instead of four hundred and one, with the
  golden-section search refining inside one step of that grid. A million-row
  fit no longer spends its whole budget in the profile likelihood, and the
  estimate is the same.
- The interface no longer opens files, reads the environment or starts a
  process: `fp-app/src/platform.rs` installs the four helpers in
  `fp_ui::Storage`, and the preferences, the temporary print path and the
  diagnostics log go through them. Without a desktop, the interface reports
  that nothing was written instead of failing silently.
- A second gate measures the analyses dominated by an iterative fit
  (distribution comparison, Box–Cox transformation, stepwise regression,
  ordinal and Poisson regression, accelerated life) at a hundred thousand rows
  with its own budget, and the worksheet frame and the cell edit are budgeted
  with a million rows as well as a hundred thousand.

### Fixed

- Formula and workbook readers now refuse hostile inputs instead of exhausting
  memory or the stack: nested expressions are depth-limited, huge ranges are
  clamped to the worksheet, oversized cell references are rejected and a
  decompressed workbook entry cannot exceed its declared size.
- Engine errors no longer carry the misleading "unsupported option:" prefix;
  the message states the actual cause, such as a division by zero or a missing
  column.
- The correlation analysis no longer stalls on large worksheets: Kendall's
  tau-b now counts its pairs in `O(n log n)` instead of comparing every pair.
- The Wilcoxon signed-rank report no longer builds every Walsh average on a
  large sample: past 4 000 000 pairs the median interval is omitted with an
  explanatory note instead of exhausting memory.
- The centred moving average slides its window instead of re-summing it, so
  the cost is linear in the series length and no longer grows with the window.
- The correlation matrix refuses more than 40 000 000 pair-observations
  (column pairs × rows) with a message asking for fewer columns, instead of
  holding the worker for minutes on a wide selection.
- A project whose missing-value mask or column lengths do not match is now
  refused when it is read, instead of loading a table that could panic when a
  cell was displayed.
- A workbook that claims millions of padded cells (a cell in the last Excel
  column on many rows) is refused once the five-million-cell budget is
  exceeded, instead of allocating until the process is killed.
- The Kaplan–Meier fit no longer rescans the sample for every event time: the
  at-risk count advances with the sorted observations, so a million distinct
  times take linear time after the sort.
- Box-Cox capability now says that the specification limits must be positive,
  instead of reporting a non-positive observation when a limit is zero.
- Holt–Winters smoothing uses a coarser grid search above 100 000
  observations, so a million-row series no longer spends ten seconds choosing
  the constants; the report states it.
- Very large plots are thinned to a bounded sample (one in every k
  observations) and the report says so: the scatterplot, dotplot, time-series,
  probability and empirical-CDF plots, matrix panels, run chart and control
  charts no longer carry a point for every one of a million observations, and
  the time-series and survival tables list at most 500 rows. The statistics
  still use every observation.

## [0.1.0] - 2026-11-08

### Added

- One archive per platform (Windows x86_64 and Raspberry Pi 4, aarch64), with
  the licence inventory, third-party notices and SPDX SBOM inside.
- An unsigned Windows executable and a `SHA256SUMS` file in every release.
- Expected-PPM confidence intervals for the normal capability report (overall
  and within sigma), and the reference-corpus provenance report.

### Notes

- The Windows executable is unsigned for now: SmartScreen may warn on a
  downloaded copy. Verify the SHA-256 against `SHA256SUMS` in the release before
  running it.

### Fixed

- Closing the window with unsaved changes now asks before discarding them and
  writes a recovery copy in `FreePlots-data`.
- The formula bar exposes the accessible name "Formula bar" to screen readers.
- The Taguchi level calculation names its XOR variable correctly.
