#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check that the relative links in the documents point at a real file.

A README that points at a deleted file is a broken first impression.  This
check reads every committed Markdown file except the licence inventory and the
generated third-party notice, extracts the inline and image links, skips the
ones that leave the repository (http, https, mailto, absolute paths, anchors)
and fails when a relative target does not exist.

    python3 scripts/check-links.py
    python3 scripts/check-links.py --self-test

Code fences are skipped, so an example that contains a link is not a document
link.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"!?\[[^\]]*\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+\"[^\"]*\")?\s*\)")
SKIP_PREFIXES = ("http://", "https://", "mailto:", "tel:", "data:", "//")
SKIP_FILES = {"THIRD-PARTY.md"}
SKIP_DIRS = {".git", "licenses", "target"}


def links(text: str) -> list[tuple[int, str]]:
    """Every relative link target, with its 1-based line number."""
    found = []
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.lstrip()
        if stripped.startswith("```") or stripped.startswith("~~~"):
            marker = stripped[:3]
            fence = None if fence == marker else (fence or marker)
            continue
        if fence is not None:
            continue
        for match in LINK.finditer(line):
            target = match.group(1)
            if target.startswith("<") and target.endswith(">"):
                target = target[1:-1]
            target = target.strip()
            if (
                not target
                or target.startswith("#")
                or target.startswith("/")
                or target.startswith(SKIP_PREFIXES)
            ):
                continue
            found.append((number, target))
    return found


def markdown_files() -> list[Path]:
    try:
        output = subprocess.run(
            ["git", "ls-files", "-z", "*.md"],
            cwd=ROOT,
            capture_output=True,
            check=True,
        ).stdout
        return [Path(name.decode("utf-8")) for name in output.split(b"\0") if name]
    except (OSError, subprocess.CalledProcessError):
        return [
            path.relative_to(ROOT)
            for path in ROOT.rglob("*.md")
            if not any(part in SKIP_DIRS for part in path.parts)
        ]


def resolve(document: Path, target: str) -> Path:
    path = unquote(target.split("#", 1)[0].split("?", 1)[0])
    return (ROOT / document.parent / path).resolve()


def self_test() -> None:
    sample = (
        "[a](one.md) [b](https://example.org) [c](#anchor) "
        "[d](<two words.md>) ![e](images/pic.png)\n"
        "```\n[f](inside-fence.md)\n```\n"
        "[g](three.md#part)\n"
    )
    targets = [target for _, target in links(sample)]
    expected = ["one.md", "two words.md", "images/pic.png", "three.md#part"]
    if targets != expected:
        raise SystemExit(f"self-test failed: {targets!r}")
    print("self-test: link extraction and fence handling are correct")


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        self_test()
        return 0

    failures = []
    for document in markdown_files():
        if document.name in SKIP_FILES:
            continue
        if any(part in SKIP_DIRS for part in document.parts):
            continue
        full = ROOT / document
        try:
            text = full.read_text(encoding="utf-8")
        except OSError as error:
            failures.append(f"cannot read {document.as_posix()}: {error}")
            continue
        for number, target in links(text):
            if not resolve(document, target).exists():
                failures.append(f"{document.as_posix()}:{number}: missing target {target}")

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(f"{len(failures)} broken relative link(s)", file=sys.stderr)
        return 1
    print("every relative document link points at a file")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
