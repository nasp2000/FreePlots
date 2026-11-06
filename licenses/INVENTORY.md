# Licence inventory

This directory is generated from the locked Cargo graph. Do not edit the
inventory by hand; change the dependencies and regenerate:

```bash
python3 scripts/licence-inventory.py
python3 scripts/licence-inventory.py --check
```

| File | Contents |
|---|---|
| `DEPENDENCIES.csv` | One row per crate: role, platforms, declared licence, licence actually used, authors, repository and source |
| `sbom.spdx.json` | SPDX 2.3 document with a package entry and a `DESCRIBES` relationship for every row |
| `texts/` | Canonical licence texts referenced by the generated notices |

The generated `THIRD-PARTY.md` at the repository root is the human-readable
inventory; `crates/fp-ui/assets/THIRD-PARTY-NOTICES.txt` is the self-contained
notice embedded in the binary and shipped as `LICENSES.txt`.

Roles:

- **runtime**: reachable from the CLI with default features;
- **desktop**: additional packages linked when the `desktop` feature is on;
- **build**: proc-macros and build helpers, never linked into the artifact;
- **development**: test-only packages, never shipped.

The CI `licenses` job runs `cargo deny check licenses advisories` and
`python3 scripts/licence-inventory.py --check`. Each binary build also generates
a target-specific SBOM and notice, uploaded next to the executable.
