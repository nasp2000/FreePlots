# Contributing to FreePlots

Thanks for helping. FreePlots is a statistics engine with a desktop worksheet,
so a change is only finished when the numbers, the report and the UI all agree.

## Before you start

- Open an issue for a new analysis, a behaviour change or a large refactor;
  small fixes can go straight to a pull request.
- Never paste data that you are not allowed to share. Reduce a failing data set
  to the smallest example that still shows the problem.
- Keep the engine (`fp-core`) free of operating-system calls. File dialogs,
  fonts and paths belong to `fp-app`.

## What a good change contains

1. **The formula and the convention.** State the estimator, the defaults, the
   exclusions and the confidence level in the report notes.
2. **Input handling and tests**: typical, boundary (empty, constant, n = 1,
   ties, missing values) and invalid input; the public suite runs on every
   change.
3. **The UI path** when the analysis is reachable from a dialog: the dialog,
   its validation and its persistence.
4. **Documentation**: the report note and the relevant section of the user
   documentation.

## Build

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --features desktop -- -D warnings
cargo test --workspace --features desktop --locked
```

The desktop feature is opt-in so the engine and the headless UI layer build on
machines without X11/OpenGL.

## Style

- English identifiers, English UI strings through the catalogue; Portuguese
  translations live in `crates/fp-ui/src/catalog.rs`.
- No `unwrap()` or `expect()` on paths that touch user data; return a message
  that names the column or the value.
- No new dependency without a reason and a permissive licence; run
  `cargo deny check`.
- No third-party product names, paths or copied files inside the source tree.

## Pull requests

- One topic per pull request.
- Describe the problem, the change and how you verified it.
- CI must be green before merge.

## Code of Conduct

Participation is covered by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
