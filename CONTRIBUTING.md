# Contributing to FreePlots

Thanks for helping. FreePlots is a statistics engine with a desktop worksheet, so a change is only finished when the numbers, the report and the UI all agree.

## Before you start

- Open an issue for a new analysis, a behaviour change or a large refactor; small fixes can go straight to a pull request.
- Never paste data that you are not allowed to share. Reduce a failing data set to the smallest example that still shows the problem.
- Keep the engine (`fp-core`) free of operating-system calls. File dialogs, fonts and paths belong to `fp-app`.

## What a good change contains

1. **The formula and the convention.** State the estimator, the defaults, the exclusions and the confidence level in the report notes.
2. **Tests for three variations**: typical input, boundary input (empty, constant, n = 1, ties, missing values) and invalid input.
3. **A regression fixture** when the change touches a captured or oracle-verified analysis.
4. **The UI path** when the analysis is reachable from a dialog: the dialog, its validation and its persistence.
5. **Documentation**: the report note and the relevant section of `report.md` or `docs/`.

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --features desktop -- -D warnings
cargo test --workspace --features desktop
```

The desktop feature is opt-in so the engine and the headless UI tests build on machines without X11/OpenGL.

## Style

- English identifiers, English UI strings through the catalogue; Portuguese translations live in `crates/fp-ui/src/catalog.rs`.
- No `unwrap()` or `expect()` on paths that touch user data; return a message that names the column or the value.
- No new dependency without a reason and a permissive licence; run `cargo deny check`.
- No reference-product names, paths or captured files inside the source tree.

## Pull requests

- One topic per pull request.
- Describe the problem, the change and how you tested it.
- If a number changes, include the old and the new value and the oracle or capture that justifies it.
- CI must be green before merge.

## Code of Conduct

Participation is covered by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
