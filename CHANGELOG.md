# Changelog

All notable changes to FreePlots are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
  and the panels without degrees of freedom.

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
