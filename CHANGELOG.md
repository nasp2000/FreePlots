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
- The million-row performance gate now covers the one-sample t test,
  correlation, normality and run charts, and a one-million-row CSV import.

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
- Very large plots are thinned to a bounded sample (one in every k
  observations) and the report says so; the scatterplot, dotplot, time-series
  plots and moving-average table no longer carry a point or a row for every
  one of a million observations. The statistics still use every observation.

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
