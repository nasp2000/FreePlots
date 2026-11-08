# Changelog

All notable changes to FreePlots are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-08

### Added

- Portable Windows x86_64 and Linux aarch64 executables, with the licence
  inventory, third-party notices and SPDX SBOM next to each build.
- Signed Windows executable and a `SHA256SUMS` file in every release.
- Expected-PPM confidence-interval record for normal capability, and the
  reference-corpus provenance report.

### Fixed

- Closing the window with unsaved changes now asks before discarding them and
  writes a recovery copy in `FreePlots-data`.
- The formula bar exposes the accessible name "Formula bar" to screen readers.
- The Taguchi level calculation names its XOR variable correctly.
