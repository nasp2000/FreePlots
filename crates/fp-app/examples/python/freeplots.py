"""Python bridge to the FreePlots command line.

The module only uses the standard library. It runs the executable with the
arguments of a command and parses the **Session text** the command prints: a
statistics line looks like ``  Mean                    10.000000``, so the value
is the last token of the line whose label matches.

Set ``FREEPLOTS_BIN`` to the executable and run one command per call:

>>> from freeplots import run, statistic
>>> session = run("ttest1", "weights.csv", "--column", "weight", "--mean", "9.5")
>>> statistic(session, "Estimate")
10.0
"""

from __future__ import annotations

import os
import re
import subprocess

__all__ = ["binary", "run", "statistic", "table_row", "table_value"]


def binary() -> str:
    """The executable, from FREEPLOTS_BIN (default: FreePlots on PATH)."""
    return os.environ.get("FREEPLOTS_BIN", "FreePlots")


def run(*args: object, timeout: int = 300) -> str:
    """Runs one CLI command and returns its Session text.

    Raises RuntimeError when the command fails; the message carries the
    executable's stderr, where every rejection is explained.
    """
    command = [binary(), *(str(value) for value in args)]
    result = subprocess.run(
        command, capture_output=True, text=True, timeout=timeout, check=False
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"FreePlots failed with {result.returncode}: {result.stderr.strip()}"
        )
    return result.stdout


_NUMBER = r"(-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?)"


def statistic(session: str, label: str) -> float:
    """The value of a statistics line such as ``Mean  10.000000``."""
    pattern = re.compile(
        rf"^\s*{re.escape(label)}\s+{_NUMBER}\s*$", re.MULTILINE
    )
    match = pattern.search(session)
    if match is None:
        raise KeyError(f"no statistics line {label!r} in the session")
    return float(match.group(1))


def table_row(session: str, label: str) -> list[str]:
    """The cells of a table row, after its label (``Cp  0.77  —``)."""
    pattern = re.compile(rf"^\s*{re.escape(label)}\s+(.+?)\s*$", re.MULTILINE)
    match = pattern.search(session)
    if match is None:
        raise KeyError(f"no table row {label!r} in the session")
    return [cell for cell in re.split(r"\s{2,}", match.group(1)) if cell]


def table_value(session: str, label: str, column: int = 0) -> float:
    """One numeric cell of a table row (0 is the first cell after the label)."""
    cells = table_row(session, label)
    if column >= len(cells):
        raise IndexError(f"row {label!r} has {len(cells)} cell(s)")
    return float(cells[column])
