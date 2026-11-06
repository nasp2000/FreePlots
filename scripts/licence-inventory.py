#!/usr/bin/env python3
"""Generate the third-party inventory, the binary notice and an SPDX SBOM.

The inventory is derived from the locked Cargo graph, not from a hand-written
list: every package that can reach a shipped binary is classified as runtime
(CLI), desktop (the optional window), build-time or development-only.  The
licence expressions are read from each locked package's Cargo.toml metadata and
reduced to a permissive branch where the crate offers a choice.

Usage:
    python3 scripts/licence-inventory.py
    python3 scripts/licence-inventory.py --check
    python3 scripts/licence-inventory.py --platform x86_64-pc-windows-msvc \
        --artifact FreePlots-windows-x86_64 --output-dir target/licenses

The generated files are committed so that the public repository and every
binary carry the same inventory.  CI regenerates them with --check.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SHIPPED_PLATFORMS = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]
# Some development dependencies pull target-specific helpers (WASI, Redox).
# They are never shipped, but they still need a licence row and an SBOM entry.
EXTRA_INDEX_PLATFORMS = [
    "wasm32-wasip1",
    "wasm32-wasip2",
    "x86_64-unknown-redox",
]
WORKSPACE = {"fp-app", "fp-core", "fp-ui", "fp-mcp", "fp-automation"}

# Lockfile entries that no shipped or indexed target graph resolves.  They are
# development-only pieces for UEFI builds of getrandom, so they are never part
# of a FreePlots artifact, but the inventory still records them.
TARGET_ONLY = {
    ("r-efi", "5.3.0"): {
        "license": "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
        "authors": [],
        "repository": "https://github.com/r-efi/r-efi",
    },
    ("r-efi", "6.0.0"): {
        "license": "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
        "authors": [],
        "repository": "https://github.com/r-efi/r-efi",
    },
}

# Preference order used when a crate offers a choice of licences.  All of
# these are on the allow-list in deny.toml.
PREFERENCE = [
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "BSL-1.0",
    "Unlicense",
    "0BSD",
    "IJG",
    "Unicode-3.0",
    "OFL-1.1",
    "Ubuntu-font-1.0",
    "Apache-2.0 WITH LLVM-exception",
]

LICENSE_ORDER = [name for name in PREFERENCE] + ["MIT"]
LICENSE_TITLES = {
    "MIT": "MIT License",
    "Apache-2.0": "Apache License 2.0",
    "BSD-2-Clause": "BSD 2-Clause License",
    "BSD-3-Clause": "BSD 3-Clause License",
    "ISC": "ISC License",
    "Zlib": "zlib License",
    "BSL-1.0": "Boost Software License 1.0",
    "Unlicense": "The Unlicense",
    "0BSD": "BSD Zero Clause License",
    "IJG": "Independent JPEG Group licence",
    "Unicode-3.0": "Unicode License V3",
    "OFL-1.1": "SIL Open Font License 1.1",
    "Ubuntu-font-1.0": "Ubuntu Font Licence 1.0",
    "Apache-2.0 WITH LLVM-exception": "Apache License 2.0 with LLVM exception",
}


def run_cargo(args: list[str]) -> str:
    env = dict(os.environ)
    cargo = env.get("CARGO", "cargo")
    result = subprocess.run(
        [cargo, *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(f"cargo {' '.join(args)} failed")
    return result.stdout


def metadata(platform: str) -> dict:
    text = run_cargo(
        [
            "metadata",
            "--locked",
            "--filter-platform",
            platform,
            "--features",
            "fp-app/desktop",
            "--format-version",
            "1",
        ]
    )
    return json.loads(text)


def tree(args: list[str], platform: str | None = None) -> set[tuple[str, str]]:
    command = ["tree", "--locked"]
    if platform:
        command += ["--target", platform]
    command += args + ["--prefix", "none"]
    text = run_cargo(command)
    packages: set[tuple[str, str]] = set()
    for line in text.splitlines():
        line = line.strip()
        line = re.sub(r" \(\*\)$", "", line)
        line = re.sub(r" \(proc-macro\)$", "", line)
        line = re.sub(r" \([^)]*\)$", "", line)
        parts = line.split()
        if len(parts) < 2 or parts[0] in WORKSPACE:
            continue
        packages.add((parts[0], parts[1].removeprefix("v")))
    return packages


def normalize(expr: str) -> str:
    expr = expr.strip()
    expr = expr.replace("MIT/Apache-2.0", "MIT OR Apache-2.0")
    expr = expr.replace("Unlicense/MIT", "Unlicense OR MIT")
    # A few manifests use "A / B" instead of the SPDX "A OR B" form.
    expr = re.sub(r"\s*/\s*", " OR ", expr)
    return expr


def split_top(expr: str, operator: str) -> list[str]:
    parts: list[str] = []
    depth = 0
    start = 0
    i = 0
    while i < len(expr):
        char = expr[i]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
        elif depth == 0 and expr.startswith(operator, i):
            parts.append(expr[start:i].strip())
            i += len(operator)
            start = i
            continue
        i += 1
    parts.append(expr[start:].strip())
    return [part for part in parts if part]


def strip_outer(expr: str) -> str:
    expr = expr.strip()
    while expr.startswith("(") and expr.endswith(")"):
        depth = 0
        balanced = True
        for index, char in enumerate(expr):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0 and index != len(expr) - 1:
                    balanced = False
                    break
        if not balanced:
            break
        expr = expr[1:-1].strip()
    return expr


def choose_license(expr: str) -> str:
    expr = normalize(expr)
    chosen: list[str] = []
    for conjunction in split_top(expr, " AND "):
        term = strip_outer(conjunction)
        alternatives = [strip_outer(alt) for alt in split_top(term, " OR ")]
        pick = next((candidate for candidate in PREFERENCE if candidate in alternatives), None)
        if pick is None:
            if len(alternatives) == 1:
                pick = alternatives[0]
            else:
                raise SystemExit(
                    f"no permissive branch in {expr!r} (alternatives: {alternatives})"
                )
        chosen.append(pick)
    return " AND ".join(chosen)


def license_text(identifier: str) -> str:
    path = ROOT / "licenses/texts" / f"{identifier}.txt"
    if not path.exists():
        raise SystemExit(f"missing licence text for {identifier}: {path}")
    return path.read_text().strip()


def lock_digest() -> str:
    data = (ROOT / "Cargo.lock").read_bytes()
    return hashlib.sha256(data).hexdigest()


def registry_package(key: tuple[str, str]) -> dict | None:
    """Read a locked package that cargo metadata filtered out for every target.

    A few development-only edges (for example the UEFI pieces of getrandom)
    exist in the lockfile but in no shipped target graph.  Their Cargo.toml is
    still the authoritative licence declaration, so read it from the registry
    cache.  CI runs `cargo fetch --locked` before --check.
    """
    name, version = key
    if key in TARGET_ONLY:
        known = TARGET_ONLY[key]
        return {
            "name": name,
            "version": version,
            "license": known["license"],
            "authors": known["authors"],
            "repository": known["repository"],
            "homepage": None,
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        }
    registries = Path.home() / ".cargo" / "registry" / "src"
    if not registries.exists():
        return None
    for registry in registries.iterdir():
        directory = registry / f"{name}-{version}"
        manifest = directory / "Cargo.toml"
        if not manifest.exists():
            continue
        data = tomllib.loads(manifest.read_text())
        package = data.get("package", {})
        return {
            "name": package.get("name", name),
            "version": package.get("version", version),
            "license": package.get("license"),
            "authors": package.get("authors", []),
            "repository": package.get("repository"),
            "homepage": package.get("homepage"),
            "source": "registry+https://github.com/rust-lang/crates.io-index",
        }
    return None


def copyright_notice(name: str, version: str, authors: str) -> str:
    """Best available copyright line for a package.

    Cargo.toml often omits authors, while the crate's LICENSE/COPYING file still
    carries the required notice.  Read the first copyright lines from the
    registry cache; fall back to the declared authors and then to the crate name.
    """
    if authors:
        return authors
    registries = Path.home() / ".cargo" / "registry" / "src"
    if registries.exists():
        for registry in registries.iterdir():
            directory = registry / f"{name}-{version}"
            if not directory.is_dir():
                continue
            candidates = sorted(
                path
                for path in directory.iterdir()
                if path.is_file()
                and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE"))
            )
            for path in candidates:
                try:
                    text = path.read_text(errors="replace")
                except OSError:
                    continue
                lines = text.splitlines()
                found: list[str] = []
                for index, line in enumerate(lines):
                    stripped = line.strip()
                    if re.match(r"^(?:Copyright|COPYRIGHT|copyright|©)\s*(?:\(c\)|\(C\)|©)?\s*(?:\d{4}|[A-Z])", stripped):
                        found.append(stripped)
                        if (
                            index + 1 < len(lines)
                            and "all rights reserved" in lines[index + 1].lower()
                        ):
                            found.append(lines[index + 1].strip())
                if found:
                    return "; ".join(dict.fromkeys(found))[:400]
    return f"the {name} contributors"


def package_index(platforms: list[str]) -> dict[tuple[str, str], dict]:
    index: dict[tuple[str, str], dict] = {}
    for platform in sorted(set(platforms) | set(EXTRA_INDEX_PLATFORMS)):
        for package in metadata(platform)["packages"]:
            index[(package["name"], package["version"])] = package
    return index


def collect(platforms: list[str]) -> dict[tuple[str, str], dict]:
    index = package_index(platforms)
    cli: dict[str, set] = {}
    desktop: dict[str, set] = {}
    compile_only: set[tuple[str, str]] = set()
    for platform in platforms:
        cli_all = tree(["-p", "fp-app", "-e", "normal"], platform)
        cli[platform] = tree(["-p", "fp-app", "-e", "normal,no-proc-macro"], platform)
        desktop_all = tree(
            ["-p", "fp-app", "--features", "desktop", "-e", "normal"], platform
        )
        desktop[platform] = tree(
            ["-p", "fp-app", "--features", "desktop", "-e", "normal,no-proc-macro"],
            platform,
        )
        compile_only |= (cli_all - cli[platform]) | (desktop_all - desktop[platform])
    # Build and development sets are host-side; fix the target so the committed
    # inventory is identical on Linux, Windows and macOS CI runners.
    build = tree(
        ["--workspace", "--all-targets", "-e", "build,normal,no-proc-macro"], SHIPPED_PLATFORMS[0]
    )
    dev = tree(["--workspace", "--all-targets", "-e", "dev,normal,no-proc-macro"], SHIPPED_PLATFORMS[0])

    cli_union = set().union(*cli.values())
    desktop_union = set().union(*desktop.values())
    rows: dict[tuple[str, str], dict] = {}
    for key in sorted(cli_union | desktop_union | compile_only | build | dev):
        package = index.get(key) or registry_package(key)
        if package is None:
            raise SystemExit(f"cargo metadata did not describe {key[0]} {key[1]}")
        platforms_here = {
            platform
            for platform in platforms
            if key in cli[platform]
            or key in desktop[platform]
            or (key in compile_only and platform in platforms)
        }
        if key in cli_union:
            role = "runtime"
        elif key in desktop_union:
            role = "desktop"
        elif key in compile_only or key in build:
            role = "build"
        elif key in dev:
            role = "development"
        else:
            continue
        declared = normalize(package.get("license") or "")
        if not declared:
            raise SystemExit(f"{key[0]} {key[1]} declares no licence")
        used = choose_license(declared)
        rows[key] = {
            "name": key[0],
            "version": key[1],
            "role": role,
            "platforms": platforms_here,
            "declared": declared,
            "used": used,
            "authors": ", ".join(package.get("authors") or []),
            "copyright": copyright_notice(
                key[0], key[1], ", ".join(package.get("authors") or [])
            ),
            "repository": package.get("repository")
            or package.get("homepage")
            or package.get("source")
            or "",
            "source": package.get("source") or "",
        }
    return rows


def platform_label(platforms: set[str], all_platforms: list[str]) -> str:
    if not platforms:
        return "host"
    if platforms == set(all_platforms):
        return "all"
    linux = any("linux" in platform for platform in platforms)
    windows = any("windows" in platform for platform in platforms)
    parts = []
    if linux:
        parts.append("Linux")
    if windows:
        parts.append("Windows")
    return " + ".join(parts) if parts else "host"


def markdown_table(rows: list[dict], all_platforms: list[str]) -> str:
    lines = [
        "| Crate | Version | Licence used | Declared | Platforms |",
        "|---|---|---|---|---|",
    ]
    for row in rows:
        declared = row["declared"] if row["declared"] != row["used"] else ""
        lines.append(
            f"| `{row['name']}` | {row['version']} | {row['used']} | "
            f"{declared} | {platform_label(row['platforms'], all_platforms)} |"
        )
    return "\n".join(lines)


def render_third_party(rows: dict[tuple[str, str], dict], platforms: list[str]) -> str:
    ordered = sorted(rows.values(), key=lambda row: (row["name"], row["version"]))
    by_role = {
        role: [row for row in ordered if row["role"] == role]
        for role in ("runtime", "desktop", "build", "development")
    }
    used = sorted(
        {identifier for row in ordered for identifier in row["used"].split(" AND ")},
        key=lambda identifier: LICENSE_ORDER.index(identifier)
        if identifier in LICENSE_ORDER
        else 99,
    )
    lines = [
        "# Third-party dependencies",
        "",
        "FreePlots itself is MIT licensed. This file is generated from the locked",
        "Cargo graph by `scripts/licence-inventory.py`; do not edit it by hand.",
        "It lists every crate that can reach a shipped binary and separates the",
        "always-linked runtime, the optional desktop window, build-time helpers and",
        "development-only packages.",
        "",
        f"- Lockfile: `Cargo.lock` SHA-256 `{lock_digest()}`",
        f"- Build targets: {', '.join(f'`{platform}`' for platform in platforms)}",
        "- Policy: `deny.toml`; CI runs `cargo deny check licenses advisories` and",
        "  regenerates this file with `python3 scripts/licence-inventory.py --check`.",
        "- Every dependency uses a permissive licence. Where a crate offers a choice",
        "  that includes a copyleft branch, the right-hand column records the",
        "  permissive branch FreePlots uses.",
        "",
        "## Summary",
        "",
        "| Role | Packages |",
        "|---|---:|",
    ]
    for role, title in (
        ("runtime", "Runtime (always linked into the CLI)"),
        ("desktop", "Desktop (linked when the window feature is on)"),
        ("build", "Build time (proc-macros and helpers; never linked)"),
        ("development", "Development only (never shipped)"),
    ):
        lines.append(f"| {title} | {len(by_role[role])} |")
    lines += [
        "",
        "## Runtime (always linked into the CLI)",
        "",
        markdown_table(by_role["runtime"], platforms),
        "",
        "## Desktop (linked when the window feature is on)",
        "",
        markdown_table(by_role["desktop"], platforms),
        "",
        "## Build time (proc-macros and helpers; never linked into a shipped binary)",
        "",
        markdown_table(by_role["build"], platforms),
        "",
        "## Development only (never shipped)",
        "",
        markdown_table(by_role["development"], platforms),
        "",
        "Per-package authors and repositories are in `licenses/DEPENDENCIES.csv`;",
        "the machine-readable inventory is `licenses/sbom.spdx.json`.",
        "",
        "## Licence texts",
        "",
    ]
    for identifier in used:
        lines += [
            f"### {LICENSE_TITLES.get(identifier, identifier)}",
            "",
            "```text",
            license_text(identifier),
            "```",
            "",
        ]
    return "\n".join(lines).rstrip() + "\n"


def render_notice(rows: dict[tuple[str, str], dict], platforms: list[str]) -> str:
    shipped = sorted(
        (row for row in rows.values() if row["role"] in ("runtime", "desktop")),
        key=lambda row: (row["name"], row["version"]),
    )
    used = sorted(
        {identifier for row in shipped for identifier in row["used"].split(" AND ")},
        key=lambda identifier: LICENSE_ORDER.index(identifier)
        if identifier in LICENSE_ORDER
        else 99,
    )
    font_license = (ROOT / "crates/fp-ui/assets/LICENSE-DejaVu.txt").read_text().strip()
    lines = [
        "FreePlots - third-party notices",
        "================================",
        "",
        "FreePlots is an independent open-source project. It is not affiliated with",
        "or endorsed by any commercial statistical software vendor, and it is not",
        "sponsored or approved by one. Names and marks of other products belong to",
        "their respective owners.",
        "",
        "FreePlots itself",
        "----------------",
        (ROOT / "LICENSE").read_text().strip(),
        "",
        "Third-party dependencies",
        "------------------------",
        "This binary links the permissive packages below. The author line is the",
        "copyright attribution declared by each package. The full inventory,",
        "including build-time and development-only packages, is in THIRD-PARTY.md",
        "and DEPENDENCIES.csv, shipped beside this file with every release. The",
        "licence texts follow the package list.",
        "",
    ]
    for row in shipped:
        lines.append(
            f"  {row['name']} {row['version']} - {row['used']} - {row['copyright']}"
        )
    lines += [
        "",
        "Bundled fonts",
        "-------------",
        "The interface embeds three unmodified DejaVu faces: DejaVu Sans,",
        "DejaVu Serif and DejaVu Sans Mono. They are distributed under the",
        "Bitstream Vera licence reproduced below. DejaVu changes are in the",
        "public domain.",
        "",
        font_license,
        "",
        "Licence texts",
        "-------------",
        "",
    ]
    for identifier in used:
        lines += [
            f"--- {LICENSE_TITLES.get(identifier, identifier)} ---",
            "",
            license_text(identifier),
            "",
        ]
    return "\n".join(lines).rstrip() + "\n"


def render_csv(rows: dict[tuple[str, str], dict], platforms: list[str]) -> str:
    import io

    buffer = io.StringIO()
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(
        [
            "crate",
            "version",
            "role",
            "platforms",
            "declared_license",
            "license_used",
            "authors",
            "copyright",
            "repository",
            "source",
        ]
    )
    for row in sorted(rows.values(), key=lambda row: (row["name"], row["version"])):
        writer.writerow(
            [
                row["name"],
                row["version"],
                row["role"],
                platform_label(row["platforms"], platforms),
                row["declared"],
                row["used"],
                row["authors"],
                row["copyright"],
                row["repository"],
                row["source"],
            ]
        )
    return buffer.getvalue()


def spdx_id(row: dict) -> str:
    raw = f"Package-{row['name']}-{row['version']}"
    return "SPDXRef-" + re.sub(r"[^A-Za-z0-9.-]", "-", raw)


def render_sbom(
    rows: dict[tuple[str, str], dict],
    platforms: list[str],
    artifact: str,
    created: str,
) -> str:
    package_ids: dict[tuple[str, str], str] = {}
    packages = []
    for key, row in sorted(rows.items()):
        identifier = spdx_id(row)
        while identifier in package_ids.values():
            identifier += "-x"
        package_ids[key] = identifier
        if row["source"].startswith("registry+"):
            download = (
                f"https://crates.io/api/v1/crates/{row['name']}/{row['version']}/download"
            )
        elif row["source"].startswith("git+"):
            download = row["source"].removeprefix("git+").split("#", 1)[0]
        else:
            download = "NOASSERTION"
        packages.append(
            {
                "SPDXID": identifier,
                "name": row["name"],
                "versionInfo": row["version"],
                "downloadLocation": download,
                "filesAnalyzed": False,
                "licenseConcluded": row["used"],
                "licenseDeclared": row["declared"],
                "copyrightText": row["copyright"],
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceType": "purl",
                        "referenceLocator": f"pkg:cargo/{row['name']}@{row['version']}",
                    }
                ],
                "comment": (
                    f"roles={row['role']}; "
                    f"platforms={platform_label(row['platforms'], platforms)}"
                ),
            }
        )
    namespace = (
        "https://github.com/nasp2000/FreePlots/sbom/"
        f"{lock_digest()[:16]}/{re.sub(r'[^A-Za-z0-9.-]', '-', artifact)}"
    )
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": artifact if artifact.startswith("FreePlots") else f"FreePlots-{artifact}",
        "documentNamespace": namespace,
        "creationInfo": {
            "created": created,
            "creators": [
                "Tool: freeplots-licence-inventory",
                "Organization: FreePlots contributors",
            ],
        },
        "packages": packages,
        "relationships": [
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": package["SPDXID"],
            }
            for package in packages
        ],
    }
    return json.dumps(document, indent=2, sort_keys=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--platform",
        action="append",
        dest="platforms",
        help="build target to include (repeatable; default: all shipped targets)",
    )
    parser.add_argument(
        "--artifact",
        default="all-targets",
        help="name used in the SBOM document (default: all-targets)",
    )
    parser.add_argument("--output-dir", default=str(ROOT / "licenses"))
    parser.add_argument("--third-party", default=str(ROOT / "THIRD-PARTY.md"))
    parser.add_argument(
        "--notice",
        default=str(ROOT / "crates/fp-ui/assets/THIRD-PARTY-NOTICES.txt"),
    )
    parser.add_argument("--created", default="1970-01-01T00:00:00Z")
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail when the committed files differ from the generated ones",
    )
    args = parser.parse_args()

    platforms = args.platforms or SHIPPED_PLATFORMS
    rows = collect(platforms)
    output = {
        Path(args.third_party): render_third_party(rows, platforms),
        Path(args.notice): render_notice(rows, platforms),
        Path(args.output_dir) / "DEPENDENCIES.csv": render_csv(rows, platforms),
        Path(args.output_dir) / "sbom.spdx.json": render_sbom(
            rows, platforms, args.artifact, args.created
        ),
    }

    if args.check:
        stale = []
        for path, text in output.items():
            if not path.exists() or path.read_text() != text:
                stale.append(path.relative_to(ROOT))
        if stale:
            for path in stale:
                print(f"stale: {path}", file=sys.stderr)
            print(
                "run python3 scripts/licence-inventory.py and commit the result",
                file=sys.stderr,
            )
            return 1
        print(f"licence inventory is current ({len(rows)} packages)")
        return 0

    for path, text in output.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        try:
            shown = path.relative_to(ROOT)
        except ValueError:
            shown = path
        print(f"wrote {shown}")
    print(f"{len(rows)} packages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
