# Changelog

All notable changes to FreePlots are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- The million-row performance gate now covers descriptive statistics,
  capability (plain and Box–Cox), I–MR, X̄–R, regression (one and two
  predictors), logistic regression, ANOVA, PCA, K-means, moving average,
  time-series decomposition and smoothing, scatterplot, the one- and two-sample
  t tests, histogram, boxplot, probability plot, ECDF, correlation, normality,
  run charts and Kaplan–Meier, plus a one-million-row CSV import.

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
