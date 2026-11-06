# Validation results

Public summary of the captured-run comparisons. The raw Session text and graph
exports stay in the private capture pack; this page records only the dataset,
the analysis, the outcome and the tolerance. See `PROVENANCE.md` for the method.

## Comparison session

- Capture session: 2026-10-25, external commercial statistical package on a
  dedicated Windows workstation, English interface.
- Datasets: fixed synthetic CSVs in `datasets/`.
- Comparator: `python3 validation/compare.py`.
- Tolerances: numbers `5e-4` absolute or relative; Anderson–Darling and p-values
  `1.5e-3` absolute; `n` exact.

## Closed by the capture

| Dataset / analysis | Result | Public fixtures |
|---|---|---|
| `normal_n8`, `normal_n20`, `normal_n30`, `normal_n200` — probability plot, Normal | Pass | FreePlots Session text in `freplots/` |
| `lognormal_n30` — probability plot, Lognormal | Pass | `freplots/lognormal_n30.txt` |
| `weibull_n20` — probability plot, Weibull | Pass | `freplots/weibull_n20.txt` |
| `exponential_n20` — probability plot, Exponential | Pass | `freplots/exponential_n20.txt` |
| `normal_n20` — normality test, Anderson–Darling | Pass | `freplots/normality_n20.txt` |
| `twosample` — two-sample t (Welch) | Pass | `freplots/twosample.txt` |
| `oneway` — one-way ANOVA, including the Tukey HSD and Fisher LSD tables | Pass | `freplots/oneway.txt`; regression test `captured_validation.rs` |
| `regress` — ordinary least squares | Pass | `freplots/regress.txt` |
| `correlation` — Pearson matrix | Pass | `freplots/correlation.txt` |
| `normal_n20` — probability plot, Logistic | Pass | `freplots/logistic_n20.txt` |
| `normal_n20` — capability (LSL 15, USL 25) | Pass | `freplots/dcapa_n20.txt` |
| `normal_n20` — I–MR, X̄–R and X̄–S limits | Pass | `freplots/` |
| `normal_n20` — CUSUM default plan | Pass | `freplots/cusum_n20.txt` |
| `normal_n20` — distribution identification | Pass for the fitted table | `freplots/` |

The graph checks (ticks, range, band edges, fitted line) were performed on the
exported plots. The ticks and bands that were verified are recorded in the local
parity notes; that file also marks the cases that remain provisional.

## Captured, compared manually

These areas had a captured Session and were checked number by number because
their labels differ from the probability-plot comparator. They are not
automatic regressions yet.

| Area | Result |
|---|---|
| Lognormal, Weibull and exponential non-normal capability | Parameters, expected/observed PPM and Pp/PPL/PPU/Ppk agree; expected-PPM confidence intervals were not captured |
| Loglogistic probability plot | Agrees after the parameterisation is translated (FreePlots prints scale/shape; the capture prints the log location and reciprocal scale) |
| Logistic 95 % band | Agrees at p = 5 % and p = 10 % |
| Weibull 95 % band | Agrees within 0.3 % on the log10 axis |
| Test 1–8 control-chart signals | Limits and per-point signals agree on the captured samples |
| Grouped descriptive statistics, G chart, acceptance sampling, run chart, proportion tests, sign test, non-parametrics, two-way ANOVA, Kaplan–Meier, interval plot, Taguchi L18, Plackett–Burman, partial correlation, data manipulation | Captured in the 19-area kit; each row is compared manually and recorded in the local capture log |

## Open

| Area | Why it is open |
|---|---|
| Expected-PPM confidence interval | The capture command opened a modal dialog and the capture host stopped responding; a new capture session is needed |
| Some graph-only conventions (exact tick set for Weibull/exponential, axis padding) | The text capture cannot prove them; they are checked visually and remain provisional |
| Areas with no captured run at all | Listed with their status in `docs/parity.md`; they are covered by independent oracles or closed-form tests, not by a captured comparison |

## Interpretation

A row is marked pass only when the Session numbers are inside the tolerances and
the graph checklist is complete. Matching a formula or a common convention is
not, by itself, a captured comparison. The absence of a captured row is not
evidence of a defect; it is an explicit gap in coverage.
