#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Print the CHANGELOG section for a release tag.

Usage: python3 scripts/changelog-section.py v0.1.0 > release-notes.md
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: changelog-section.py <tag>", file=sys.stderr)
        return 2
    version = sys.argv[1].removeprefix("v")
    heading = f"## [{version}]"
    lines = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8").splitlines()
    start = next((i for i, line in enumerate(lines) if line.startswith(heading)), None)
    if start is None:
        print(f"CHANGELOG.md has no section {heading}", file=sys.stderr)
        return 1
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i].startswith("## [")),
        len(lines),
    )
    section = "\n".join(lines[start:end]).strip()
    if not section:
        print(f"CHANGELOG.md section {heading} is empty", file=sys.stderr)
        return 1
    print(section)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
