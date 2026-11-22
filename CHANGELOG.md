# Changelog

All notable changes to FreePlots are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- *Attribute Capability…* reports DPMO and yield alongside PPM, DPU and process
  Z, takes the opportunities per unit (one by default) and is reachable from
  the `attrcap` command. Process Z follows the per-opportunity rate.
- *Johnson Transformation…* takes optional specification limits and adds Pp
  and Ppk computed on the transformed scores against the transformed limits;
  the family, the limits and the choice are remembered, and the `johnson`
  command prints the fit and its capability.
- *Tolerance Interval…* keeps both limits or only one: the two-sided normal
  factor uses Howe's approximation and the one-sided factor is the exact
  non-central-t solution, while the distribution-free method takes the
  narrowest order statistics that reach the confidence. The population
  proportion and the confidence are adjustable, the dialog remembers the side,
  and the `tolerance` command gained `--side <two|lower|upper>`.
- *Acceptance Sampling…* evaluates single and double plans: the OC curve with
  the binomial or hypergeometric probability, the AQL and LTPD points with the
  producer and consumer risks, the average sample number, the average outgoing
  quality and the average total inspection. The `occurve` command gained
  `--aql`, `--ltpd` and the `--double` plan options; the dialog, the reports
  and the command are in both interface languages.

## [0.1.4] - 2026-11-21

### Fixed

- Editing the X and Y axis labels updates every panel of the graph, and the
  label editor stays above the graph it edits.
- Percentile lines can be added to a panel grid: the dialog reads the points of
  every panel instead of refusing the values.
- The toolbar column chooser moves the worksheet selection, a greyed toolbar
  icon no longer reacts to clicks, turning native plot windows off closes the
  graph left in the old window, and the *Window* menu opens the graph the last
  analysis drew.
- The Open and Save dialogs stay open when the path is empty, Escape cancels a
  pending close and the replace prompt, and a folder with the destination name
  is no longer announced as a file to replace.
- Opening a project replaces the whole workspace: a one-sheet project no longer
  leaves the previous worksheet behind.
- Escape closes the settings, diagnostics, licences, agents, tour and
  sheet-rename windows.
- Redo repeats an undone edit, and *Predict…* is reached under
  *Stat > Regression*.

## [0.1.3] - 2026-11-20

### Fixed

- A project with empty cells opens again: the missing values were written as
  `null` and refused when the file was read.
- The Z and Poisson dialog stays open when a value is rejected and keeps what
  was typed.
- The *Store Descriptive Statistics* and *Tally* dialogs keep their buttons
  inside the window on a worksheet with many columns.
- A graph opens above the window that produced it and takes the focus; the
  *Window > Probability Plot* entry raises it again.

## [0.1.2] - 2026-11-18

### Added

- **Calc:** data calculations (standardize, column and row statistics,
  patterned and mesh data, indicators, transpose, matrix product), random
  data, bootstrap and a randomization test.
- **Regression:** CART and random forests (with cross-validation), nonlinear
  regression, best subsets, prediction and partial least squares.
- **Basic Statistics:** power and sample size (including the linear-model
  partial F test), 2×2 crossover equivalence, runs, Z and Poisson tests,
  graphical summary, covariance and tally.
- **ANOVA:** general linear model, MANOVA, nested ANOVA, analysis of means and
  two-way ANOVA.
- **Reliability:** survival analysis (exponential, Weibull and lognormal with
  AIC/BIC, percentiles and mission metrics).
- **Time series:** ARIMA and diagnostics (ACF/PACF, cross-correlation, unit
  root, differences and trends).
- **DOE:** general factorial designs and analysis, screening, optimal and
  mixture designs, factorial plots.
- **Quality:** capability sixpack, attribute capability, T chart, CUSUM and
  acceptance charts, Johnson transformation, variables sampling, Gage R&R,
  attribute agreement and distribution identification.
- **Multivariate and graphs:** generalized variance and MEWMA charts, a
  probability distribution plot and nine descriptive charts.
- Help pages in English and Portuguese for the new analyses, with CLI and MCP
  coverage.

### Changed

- Mesh and indicator order follow the reference; ANOM uses the studentized
  maximum modulus quantile; the 2×2 crossover dialog can choose the reference
  treatment; bootstrap reports the percentile interval of the mean.

### Fixed

- MANOVA's Hotelling–Lawley F uses Rao's denominator degrees of freedom; the
  mesh factor fields are labelled; small dialog and test fixes.

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
