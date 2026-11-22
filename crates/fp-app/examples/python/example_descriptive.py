# SPDX-License-Identifier: MIT
"""Example: a one-sample t test through the CLI, read back from Python.

Usage: python3 example_descriptive.py <file.csv> [column]

The script prints the values the bridge read from the Session text, so another
program (or a shell test) can compare them with its own numbers:

    mean=10.000000 sd=0.866025 t=1.732051

The standard deviation is rebuilt from the standard error of the mean and the
degrees of freedom, which shows how the statistics lines are read.
"""

from __future__ import annotations

import math
import sys

from freeplots import run, statistic


def main(argv: list[str]) -> int:
    csv = argv[1] if len(argv) > 1 else "measurements.csv"
    column = argv[2] if len(argv) > 2 else "weight"
    session = run(
        "ttest1", csv, "--column", column, "--mean", "9.5", "--alternative", "two-sided"
    )
    mean = statistic(session, "Estimate")
    standard_error = statistic(session, "Standard error")
    degrees = statistic(session, "df")
    t_value = statistic(session, "t")
    deviation = standard_error * math.sqrt(degrees + 1.0)
    print(f"mean={mean:.6f} sd={deviation:.6f} t={t_value:.6f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
