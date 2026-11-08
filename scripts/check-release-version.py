#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Fail unless the release tag matches the workspace version.

Usage: python3 scripts/check-release-version.py v0.1.0
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: check-release-version.py <tag>", file=sys.stderr)
        return 2
    tag = sys.argv[1].removeprefix("v")
    with (ROOT / "Cargo.toml").open("rb") as handle:
        version = tomllib.load(handle)["workspace"]["package"]["version"]
    if tag != version:
        print(
            f"release tag is v{tag} but Cargo.toml is {version}",
            file=sys.stderr,
        )
        return 1
    print(f"release tag v{tag} matches Cargo.toml")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
