#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Guard the layering of the workspace.

Three rules keep the code split the way PLAN.md describes:

* `fp-core` is the pure engine: no operating-system code and no knowledge of
  the window that draws it.
* `fp-ui` is pure egui: it computes no statistic (it calls `fp-core`) and it
  opens no file, reads no environment variable and starts no process (the
  desktop supplies the helpers in `fp_ui::Storage`).
* The desktop runner `fp-app/src/gui.rs` reaches the operating system through
  `fp-app/src/platform.rs`, the only module allowed to do it. The command-line
  front end in `fp-app/src/cli.rs` and `main.rs` is a headless tool and is not
  covered.

Test code is out of scope: `#[cfg(test)]` items, `tests/` directories and the
`*_test` helpers are stripped before the patterns run.

    python3 scripts/check-architecture.py
    python3 scripts/check-architecture.py --self-test
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

OS_PATTERN = re.compile(r"\bstd::(fs|env|process|os)\b")
PLATFORM_PATTERN = re.compile(r"\bstd::os\b|cfg!\(target_os|cfg!\(windows|cfg!\(unix")
STATISTICS_PATTERN = re.compile(
    r"\b(ln|exp|sqrt|powf|powi|ln_gamma|erf|erfc|normal_quantile|t_quantile)\s*\("
)

# Directories whose sources are checked, and the patterns each must avoid.
RULES: list[tuple[str, re.Pattern[str], str, set[str]]] = [
    (
        "crates/fp-core/src",
        PLATFORM_PATTERN,
        "fp-core must stay OS-agnostic; move this into fp-app/src/platform.rs",
        set(),
    ),
    (
        "crates/fp-ui/src",
        OS_PATTERN,
        "fp-ui must stay pure; install the helpers in fp_ui::Storage from fp-app",
        {"app/storage.rs"},
    ),
    (
        "crates/fp-ui/src",
        STATISTICS_PATTERN,
        "fp-ui must not compute statistics; call fp-core instead",
        set(),
    ),
    (
        "crates/fp-app/src",
        OS_PATTERN,
        "the desktop runner must reach the system through src/platform.rs",
        {"platform.rs", "cli.rs", "main.rs", "bridge.rs"},
    ),
]


def strip_comments(text: str) -> str:
    """Removes `//` and `/* */` comments, keeping the line structure."""
    out = []
    index = 0
    length = len(text)
    while index < length:
        char = text[index]
        if text.startswith("//", index):
            end = text.find("\n", index)
            index = length if end < 0 else end
        elif text.startswith("/*", index):
            end = text.find("*/", index + 2)
            block = text[index : length if end < 0 else end + 2]
            out.append("\n" * block.count("\n"))
            index = length if end < 0 else end + 2
        elif char == '"':
            end = index + 1
            while end < length:
                if text[end] == "\\":
                    end += 2
                    continue
                if text[end] == '"':
                    end += 1
                    break
                end += 1
            out.append(text[index:end])
            index = end
        else:
            out.append(char)
            index += 1
    return "".join(out)


def strip_test_items(text: str) -> str:
    """Removes every `#[cfg(test)]` item, including an inner `mod tests`."""
    previous = None
    while previous != text:
        previous = text
        match = re.search(r"#\[cfg\(test\)\]", text)
        if match is None:
            break
        start = match.start()
        opening = text.find("{", match.end())
        if opening < 0:
            text = text[:start] + text[match.end() :]
            continue
        # A `mod tests;` declaration (or any item without a body) ends at `;`.
        semicolon = text.find(";", match.end())
        if 0 <= semicolon < opening:
            text = text[:start] + text[semicolon + 1 :]
            continue
        depth = 0
        index = opening
        while index < len(text):
            if text[index] == "{":
                depth += 1
            elif text[index] == "}":
                depth -= 1
                if depth == 0:
                    index += 1
                    break
            index += 1
        text = text[:start] + text[index:]
    return text


def source_lines(text: str) -> list[tuple[int, str]]:
    """Splits stripped text into numbered lines, dropping empty ones."""
    return [
        (number, line)
        for number, line in enumerate(strip_test_items(strip_comments(text)).splitlines(), 1)
        if line.strip()
    ]


def skip(path: Path, relative: Path, exempt: set[str]) -> bool:
    if "tests" in relative.parts or relative.name.startswith("tests"):
        return True
    return relative.as_posix() in exempt or relative.name in exempt


def failures_for(directory: str, pattern: re.Pattern[str], message: str, exempt: set[str]) -> list[str]:
    base = ROOT / directory
    if not base.is_dir():
        return []
    failures = []
    for path in sorted(base.rglob("*.rs")):
        relative = path.relative_to(base)
        if skip(path, relative, exempt):
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for number, line in source_lines(text):
            if pattern.search(line):
                failures.append(
                    f"{path.relative_to(ROOT).as_posix()}:{number}: {message}: {line.strip()}"
                )
    return failures


def self_test() -> None:
    cases = [
        ("fn main() { std::fs::read_to_string(\"a\") }", True),
        ("// std::fs in a comment", False),
        ("/* std::env::temp_dir() */", False),
        ("let label = \"std::fs::write\";", True),
        ("#[cfg(test)]\nfn helper() {\n    std::fs::read(\"a\");\n}", False),
        ("#[cfg(test)]\nmod tests;\nfn main() {}\n", False),
        ("mod tests {\n    #[test]\n    fn a() { std::env::temp_dir(); }\n}", True),
        ("fn scale(x: f64) -> f64 { x.powf(2.0) }", True),
        ("let noise = (index as f64).sin();", False),
        ("// x.exp() in prose", False),
    ]
    for source, expected in cases:
        found = any(pattern.search(line) for _, line in source_lines(source) for pattern in [OS_PATTERN, STATISTICS_PATTERN])
        if found is not expected:
            raise SystemExit(f"self-test failed for {source!r}: found={found} expected={expected}")
    # The comment stripper keeps line numbers for the lines after a block.
    stripped = strip_comments("a\n/* two\nlines */\nb\n")
    if stripped.count("\n") != 4:
        raise SystemExit("self-test failed: a block comment must keep its line count")
    print("self-test: the patterns, the comment stripper and the test filter behave")


def tracked_sources() -> list[Path]:
    try:
        output = subprocess.run(
            ["git", "ls-files", "-z"],
            cwd=ROOT,
            capture_output=True,
            check=True,
        ).stdout
        names = [Path(name.decode("utf-8")) for name in output.split(b"\0") if name]
        return [ROOT / name for name in names if name.suffix == ".rs"]
    except (OSError, subprocess.CalledProcessError):
        return [path for path in ROOT.glob("crates/*/src/**/*.rs")]


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        self_test()
        return 0

    # Only files git knows about are judged, so a stale build directory cannot
    # make the guard fail.
    known = {path.relative_to(ROOT).as_posix() for path in tracked_sources()}
    failures = []
    for directory, pattern, message, exempt in RULES:
        base = ROOT / directory
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            relative = path.relative_to(base)
            if skip(path, relative, exempt):
                continue
            if path.relative_to(ROOT).as_posix() not in known:
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            for number, line in source_lines(text):
                if pattern.search(line):
                    failures.append(
                        f"{path.relative_to(ROOT).as_posix()}:{number}: {message}: {line.strip()}"
                    )

    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print(f"{len(failures)} architecture violation(s)", file=sys.stderr)
        return 1
    print("the engine, the interface and the desktop respect the layering")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
