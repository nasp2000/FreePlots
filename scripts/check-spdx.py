#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check that every source file of our own code carries an SPDX licence line.

The project licence is MIT (see LICENSE).  A short `SPDX-License-Identifier:
MIT` line in each file states that per file, so a file that leaves the tree on
its own still says what it may be used for, and a future audit does not have to
guess.  Third-party texts under licenses/, documents, data files and assets
that are carried verbatim are exempt.

    python3 scripts/check-spdx.py
    python3 scripts/check-spdx.py --self-test

PowerShell scripts must also stay pure ASCII: Windows PowerShell 5.1 reads a
file without a byte order mark as ANSI, so a UTF-8 character in an expected
text never matches.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HEADER = "SPDX-License-Identifier: MIT"

# A leading `prefix` comment, and the first line that may come before it.
STYLES = {
    ".rs": ("// ", ""),
    ".toml": ("# ", ""),
    ".yml": ("# ", ""),
    ".yaml": ("# ", ""),
    ".py": ("# ", "#!"),
    ".sh": ("# ", "#!"),
    ".ps1": ("# ", ""),
    ".manifest": ("<!-- ", "<?xml"),
}

EXEMPT_NAMES = {"LICENSE", "THIRD-PARTY.md", "THIRD-PARTY-NOTICES.txt"}
EXEMPT_DIRS = {".git", "licenses", "target", "docs", "testdata"}
EXEMPT_SUFFIXES = {
    ".md",
    ".txt",
    ".csv",
    ".json",
    ".svg",
    ".png",
    ".gif",
    ".mp4",
    ".ico",
    ".ttf",
    ".rgba",
    ".xlsx",
    ".lock",
    ".spdx",
}


def tracked_files() -> list[Path]:
    try:
        output = subprocess.run(
            ["git", "ls-files", "-z"],
            cwd=ROOT,
            capture_output=True,
            check=True,
        ).stdout
        return [Path(name.decode("utf-8")) for name in output.split(b"\0") if name]
    except (OSError, subprocess.CalledProcessError):
        return [
            path.relative_to(ROOT)
            for path in ROOT.rglob("*")
            if path.is_file() and ".git" not in path.parts and "target" not in path.parts
        ]


def has_header(lines: list[str], after: str) -> bool:
    start = 1 if after and lines and lines[0].startswith(after) else 0
    return any(HEADER in line for line in lines[start : start + 4])


def check_file(path: Path, text: str) -> str | None:
    lines = text.splitlines()
    prefix, after = STYLES[path.suffix]
    if not has_header(lines, after):
        return f"missing {HEADER}: {path.as_posix()}"
    if path.suffix == ".ps1":
        for number, line in enumerate(text.splitlines(), 1):
            if any(ord(character) > 127 for character in line):
                return f"non-ASCII byte in a PowerShell script: {path.as_posix()}:{number}"
    return None


def self_test() -> None:
    checks = [
        (["// SPDX-License-Identifier: MIT", "fn main() {}"], "", True),
        (["", "", "", "", "", "// SPDX-License-Identifier: MIT"], "", False),
        (["#!/usr/bin/env python3", "# SPDX-License-Identifier: MIT"], "#!", True),
        (["#!/usr/bin/env python3"], "#!", False),
        (["// SPDX-License-Identifier: Apache-2.0"], "", False),
    ]
    for lines, after, expected in checks:
        if has_header(lines, after) is not expected:
            raise SystemExit(f"self-test failed for {lines!r}")
    print("self-test: header detection and shebang handling are correct")


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        self_test()
        return 0

    failures = []
    for relative in tracked_files():
        if relative.name in EXEMPT_NAMES:
            continue
        if any(part in EXEMPT_DIRS for part in relative.parts):
            continue
        if relative.suffix not in STYLES or relative.suffix in EXEMPT_SUFFIXES:
            continue
        full = ROOT / relative
        try:
            text = full.read_text(encoding="utf-8")
        except OSError as error:
            failures.append(f"cannot read {relative.as_posix()}: {error}")
            continue
        failure = check_file(relative, text)
        if failure:
            failures.append(failure)

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(f"{len(failures)} file(s) without a valid SPDX line", file=sys.stderr)
        return 1
    print("every source file carries the MIT SPDX line")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
