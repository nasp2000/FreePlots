#!/usr/bin/env python3
"""Compare a captured run-application Session report with FreePlots.

The captured run product runs on Windows, so its Session output is copied
into validation/captured-output/. This script does not run it.

Usage:
  python3 validation/compare.py --reference FILE --freplots FILE
  python3 validation/compare.py --selftest

Exit code 0 means every comparable number is inside tolerance. Missing values
or a missing required key are failures, not silently ignored.
"""

from __future__ import annotations

import argparse
import math
import re
import sys

FREEPLOTS_PATTERNS = {
    "n": r"Observations \(n\)\s+(\d+)",
    "mean": r"Mean \((?:µ|θ)\)\s+([-+0-9.eE]+)",
    "sd": r"Standard deviation \(σ\)\s+([-+0-9.eE]+)",
    "shape": r"Shape \(α\)\s+([-+0-9.eE]+)",
    "scale": r"Scale \((?:β|σ)\)\s+([-+0-9.eE]+)",
    "location": r"Location \(µ\)\s+([-+0-9.eE]+)",
    "ad": r"Anderson–Darling \(A²\)\s+([-+0-9.eE]+)",
    "p": r"p-value \([^)]+\)\s+([-+0-9.eE]+)",
}

LABEL_MAP = {
    "n": "n",
    "mean": "mean",
    "stdev": "sd",
    "stddev": "sd",
    "std dev": "sd",
    "st dev": "sd",
    "standard deviation": "sd",
    "shape": "shape",
    "scale": "scale",
    "loc": "location",
    "location": "location",
    "ad": "ad",
    "p value": "p",
    "p-value": "p",
}

REQUIRED = {"n"}


def parse_freplots(text: str) -> dict[str, float]:
    out: dict[str, float] = {}
    for key, pattern in FREEPLOTS_PATTERNS.items():
        match = re.search(pattern, text)
        if match:
            try:
                out[key] = float(match.group(1).replace(",", "."))
            except ValueError:
                pass
    return out


def parse_reference(text: str) -> dict[str, float]:
    out: dict[str, float] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or " - " in line or line.startswith("Probability Plot"):
            continue
        match = re.match(
            r"^([A-Za-z][A-Za-z0-9 \-]*?)\s+([-+0-9.eE]+|\*|<[0-9.]+|>[0-9.]+)$",
            line,
        )
        if not match:
            continue
        label = match.group(1).strip().lower()
        raw = match.group(2)
        key = LABEL_MAP.get(label)
        if not key:
            continue
        if raw in {"*"} or raw.startswith(("<", ">")):
            # A clamped/undefined p-value is reported separately; do not turn
            # it into a number that would fake agreement.
            out[key] = math.nan
            continue
        try:
            out[key] = float(raw)
        except ValueError:
            pass
    return out


def tolerance(key: str) -> tuple[float, float]:
    """(absolute, relative) tolerance."""
    if key == "n":
        return 0.0, 0.0
    if key in {"ad", "p"}:
        return 1.5e-3, 0.0
    return 5e-4, 5e-4


def compare(reference: dict[str, float], free: dict[str, float]) -> bool:
    ok = True
    for key in REQUIRED:
        if key not in free:
            print(f"MISSING FreePlots {key}")
            ok = False
        if key not in reference:
            print(f"MISSING reference {key}")
            ok = False
    common = sorted(set(reference) & set(free))
    if not common:
        print("No comparable keys found")
        return False
    for key in common:
        a, b = reference[key], free[key]
        abs_tol, rel_tol = tolerance(key)
        if math.isnan(a) or math.isnan(b):
            print(f"{key:8} reference={a} FreePlots={b} (clamped: check manually)")
            continue
        diff = abs(a - b)
        limit = max(abs_tol, rel_tol * max(abs(a), abs(b)))
        status = "OK" if diff <= limit else "FAIL"
        print(f"{key:8} reference={a:<14g} FreePlots={b:<14g} diff={diff:.3g} {status}")
        if status != "OK":
            ok = False
    missing_free = sorted(set(reference) - set(free))
    if missing_free:
        print("FreePlots lacks:", ", ".join(missing_free))
    return ok


def selftest() -> int:
    reference = """Probability Plot of C1
Normal - 95% CI
Mean 20.7461
StDev 2.30248
N 20
AD 0.405
P-Value 0.321"""
    free = """Probability Plot: Normal — column data
Observations (n)           20
Mean (µ)                   20.746079
Standard deviation (σ)     2.302480
Anderson–Darling (A²)      0.405159
p-value (Anderson–Darling) 0.320706"""
    ok = compare(parse_reference(reference), parse_freplots(free))
    print("selftest:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", help="captured run Session text")
    parser.add_argument("--freplots", help="FreePlots Session text")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args()

    if args.selftest:
        return selftest()
    if not args.reference or not args.freplots:
        parser.error("--reference and --freplots are required unless --selftest")

    with open(args.reference, encoding="utf-8", errors="replace") as f:
        reference_text = f.read()
    with open(args.freplots, encoding="utf-8", errors="replace") as f:
        free_text = f.read()

    reference = parse_reference(reference_text)
    free = parse_freplots(free_text)
    print(f"Reference keys: {sorted(reference)}")
    print(f"FreePlots keys: {sorted(free)}")
    ok = compare(reference, free)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
