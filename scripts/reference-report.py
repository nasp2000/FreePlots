#!/usr/bin/env python3
"""Write and check the reference-corpus report.

Every committed reference file must say where its numbers come from and the
tolerance the tests apply; a reference file that no test reads, and a golden
file that no test compares against, are rejected.  This script reads that
provenance, hashes the golden plots and writes a single public report;
``--check`` fails when the report on disk no longer matches the corpus, so the
report cannot drift.

Usage:
    python3 scripts/reference-report.py            # rewrite the report
    python3 scripts/reference-report.py --check    # fail if it is stale
    python3 scripts/reference-report.py --validate # check the corpus only

The check runs in CI.  It does not run any analysis, open a network connection
or read anything outside this repository: only the committed JSON and golden
files are read.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REFERENCE = ROOT / "crates/fp-core/tests/reference"
GOLDEN = ROOT / "crates/fp-core/tests/golden"
TESTS = ROOT / "crates/fp-core/tests"
REPORT = ROOT / "docs/reference-corpus.md"

# `include_str!("reference/x.json")` and `assert_golden("x.svg", ...)`.
INCLUDE_RE = re.compile(r'include_str!\(\s*"reference/([^"]+)"\s*\)')
GOLDEN_RE = re.compile(r'assert_golden\(\s*"([^"]+)"')


def human_size(size: int) -> str:
    for unit in ("B", "KiB", "MiB"):
        if size < 1024 or unit == "MiB":
            return f"{size} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
    return f"{size} B"


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(65536), b""):
            hasher.update(block)
    return hasher.hexdigest()


def read_provenance(path: Path) -> dict:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"{path.relative_to(ROOT)}: {error}") from error
    if not isinstance(data, dict):
        raise SystemExit(f"{path.relative_to(ROOT)}: the top level is not an object")

    source = data.get("source")
    if not isinstance(source, str) or not source.strip():
        raise SystemExit(
            f"{path.relative_to(ROOT)}: missing a non-empty \"source\" string"
        )
    tolerance = data.get("tolerance")
    if isinstance(tolerance, bool) or not isinstance(tolerance, (int, float)):
        raise SystemExit(f"{path.relative_to(ROOT)}: missing a numeric \"tolerance\"")
    if tolerance <= 0:
        raise SystemExit(f"{path.relative_to(ROOT)}: \"tolerance\" must be positive")

    method = data.get("method")
    if method is not None and (not isinstance(method, str) or not method.strip()):
        raise SystemExit(f"{path.relative_to(ROOT)}: \"method\" must be a string")
    method = method or "—"

    return {
        "name": path.name,
        "source": source.strip(),
        "method": method.strip(),
        "tolerance": tolerance,
    }


def markdown_cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ").strip()


def read_usage(pattern: re.Pattern[str]) -> dict[str, list[str]]:
    """Map each included file to the test files that read it."""
    usage: dict[str, list[str]] = {}
    for path in sorted(TESTS.glob("*.rs")):
        text = path.read_text(encoding="utf-8", errors="replace")
        for name in pattern.findall(text):
            usage.setdefault(name, [])
            if path.name not in usage[name]:
                usage[name].append(path.name)
    return usage


def corpus() -> tuple[list[dict], list[Path], dict[str, list[str]]]:
    reference_files = sorted(REFERENCE.glob("*.json"))
    golden_files = sorted(path for path in GOLDEN.iterdir() if path.is_file())
    if not reference_files:
        raise SystemExit(f"no reference files under {REFERENCE.relative_to(ROOT)}")
    if not golden_files:
        raise SystemExit(f"no golden files under {GOLDEN.relative_to(ROOT)}")

    reference_usage = read_usage(INCLUDE_RE)
    golden_usage = read_usage(GOLDEN_RE)
    entries = []
    for path in reference_files:
        entry = read_provenance(path)
        if path.name not in reference_usage:
            raise SystemExit(
                f"{path.relative_to(ROOT)}: no test reads this file; "
                "remove it or include it from a test"
            )
        entry["tests"] = reference_usage[path.name]
        entries.append(entry)
    for path in golden_files:
        if path.name not in golden_usage:
            raise SystemExit(
                f"{path.relative_to(ROOT)}: no test compares against this file; "
                "remove it or call assert_golden with it"
            )
    return entries, golden_files, golden_usage


def build() -> str:
    entries, golden_files, golden_usage = corpus()
    lines = [
        "# Reference corpus",
        "",
        "This report is generated by `scripts/reference-report.py`; do not edit it",
        "by hand. It lists every committed reference file, where its numbers come",
        "from and the absolute tolerance the tests apply, followed by the golden",
        "plots and their SHA-256 digests.",
        "",
        "The tests read these files directly and never run an external program.",
        "A new reference value is accepted only with a `source` that describes the",
        "derivation, a positive `tolerance`, and (where useful) a `method` that",
        "records the formula or algorithm. Public reports describe the method; they",
        "do not name another product or tool. A reference file that no test reads,",
        "and a golden file that no test compares against, are rejected.",
        "",
        f"- reference files: **{len(entries)}**",
        f"- golden files: **{len(golden_files)}**",
        "",
        "## Reference values",
        "",
        "| File | Source | Method | Tolerance | Test |",
        "|---|---|---|---|---|",
    ]
    for entry in entries:
        tests = ", ".join(f"`{name}`" for name in entry["tests"])
        lines.append(
            f"| `{entry['name']}` "
            f"| {markdown_cell(entry['source'])} "
            f"| {markdown_cell(entry['method'])} "
            f"| `{entry['tolerance']}` "
            f"| {tests} |"
        )

    lines += [
        "",
        "## Golden plots",
        "",
        "| File | Size | SHA-256 | Test |",
        "|---|---:|---|---|",
    ]
    for path in golden_files:
        tests = ", ".join(f"`{name}`" for name in golden_usage[path.name])
        lines.append(
            f"| `{path.name}` | {human_size(path.stat().st_size)} "
            f"| `{digest(path)}` | {tests} |"
        )

    lines += [
        "",
        "## Regenerating",
        "",
        "```sh",
        "python3 scripts/reference-report.py",
        "python3 scripts/reference-report.py --check",
        "```",
        "",
        "`--check` runs in CI and fails when the corpus and this report differ.",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check",
        action="store_true",
        help="fail if the committed report is out of date instead of writing it",
    )
    mode.add_argument(
        "--validate",
        action="store_true",
        help="check the provenance and golden files without reading the report",
    )
    args = parser.parse_args()

    if args.validate:
        entries, golden_files, _ = corpus()
        for path in golden_files:
            digest(path)
        print(
            f"{len(entries)} reference files (each read by a test) and "
            f"{len(golden_files)} golden files (each compared by a test) "
            "have valid provenance"
        )
        return 0

    content = build()
    if args.check:
        current = REPORT.read_text(encoding="utf-8") if REPORT.exists() else ""
        if current != content:
            print(
                f"{REPORT.relative_to(ROOT)} is out of date; "
                "run python3 scripts/reference-report.py",
                file=sys.stderr,
            )
            return 1
        print(f"{REPORT.relative_to(ROOT)} is up to date")
        return 0

    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(content, encoding="utf-8")
    print(f"wrote {REPORT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
