#!/usr/bin/env python3
"""
Sync `crates/flint-lint/src/osquery.rs` from the canonical osquery schema.

Source: https://github.com/osquery/osquery-site (data/osquery_schema_versions/<v>.json)

Run after bumping the pinned osquery version, or whenever upstream tables change:

    python3 scripts/sync-osquery-schema.py            # default: pinned VERSION
    python3 scripts/sync-osquery-schema.py 5.23.0     # override version

The output file is fully regenerated. Fleet-specific overlay tables (e.g. the
Fleet `mdm` extension) are merged in from FLEET_OVERLAY below.
"""

from __future__ import annotations

import json
import sys
import urllib.request
from pathlib import Path

# Pin to a specific osquery schema version. Bump when Fleet bundles a newer one.
VERSION = "5.23.1"
URL_TEMPLATE = (
    "https://raw.githubusercontent.com/osquery/osquery-site/main"
    "/src/data/osquery_schema_versions/{v}.json"
)
OUT_PATH = Path(__file__).resolve().parent.parent / "crates/flint-lint/src/osquery.rs"

# Tables Fleet ships as extensions or under non-canonical names. These are
# merged into the table map after the upstream entries; on name collision the
# overlay wins (so Fleet-specific platform support overrides upstream).
FLEET_OVERLAY: list[dict] = [
    {
        "name": "atom_packages",
        "platforms": ["darwin", "linux"],
        "description": "Atom editor packages installed in a user's home directory.",
    },
    {
        "name": "filevault_status",
        "platforms": ["darwin"],
        "description": "FileVault disk encryption status (Fleet helper for macOS).",
    },
    {
        "name": "installed_applications",
        "platforms": ["darwin"],
        "description": "Installed macOS applications (Fleet alias for the `apps` table).",
    },
    {
        "name": "mdm",
        "platforms": ["darwin"],
        "description": "macOS MDM enrollment status (Fleet extension table).",
    },
]


def fetch_schema(version: str) -> list[dict]:
    url = URL_TEMPLATE.format(v=version)
    print(f"fetching {url}", file=sys.stderr)
    with urllib.request.urlopen(url) as resp:
        return json.load(resp)


def rust_string_lit(s: str) -> str:
    """Format a string as a Rust string literal, escaping " and \\."""
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def first_line(s: str) -> str:
    """Compress multi-line descriptions to a single line for the Rust source."""
    return " ".join(s.split())


def render(tables: list[dict], version: str) -> str:
    out = [
        "//! osquery table compatibility matrix.",
        "//!",
        "//! Used by `PlatformCompatibilityRule` to detect queries that reference",
        "//! tables unavailable on the declared platform.",
        "//!",
        "//! AUTO-GENERATED from the osquery upstream schema. Do not hand-edit.",
        "//! Regenerate via `python3 scripts/sync-osquery-schema.py`.",
        f"//! Schema version: {version}",
        "//! Source: https://github.com/osquery/osquery-site/tree/main/src/data/osquery_schema_versions",
        "",
        "use once_cell::sync::Lazy;",
        "use std::collections::HashMap;",
        "",
        "pub struct OsqueryTable {",
        "    pub name: &'static str,",
        "    pub platforms: Vec<&'static str>,",
        "    pub description: &'static str,",
        "}",
        "",
        f"/// {len(tables)} tables (osquery {version} + Fleet overlay).",
        "pub static OSQUERY_TABLES: Lazy<HashMap<&'static str, OsqueryTable>> = Lazy::new(|| {",
        "    let mut tables = HashMap::new();",
        "",
    ]

    for t in sorted(tables, key=lambda x: x["name"]):
        plats = ", ".join(rust_string_lit(p) for p in t["platforms"])
        out.append(f'    tables.insert(')
        out.append(f'        {rust_string_lit(t["name"])},')
        out.append(f'        OsqueryTable {{')
        out.append(f'            name: {rust_string_lit(t["name"])},')
        out.append(f'            platforms: vec![{plats}],')
        out.append(f'            description: {rust_string_lit(first_line(t["description"]))},')
        out.append(f'        }},')
        out.append(f'    );')
    out.append("")
    out.append("    tables")
    out.append("});")
    out.append("")
    return "\n".join(out)


def main() -> int:
    version = sys.argv[1] if len(sys.argv) > 1 else VERSION
    upstream = fetch_schema(version)

    # Deduplicate by name; overlay wins on collision.
    by_name: dict[str, dict] = {t["name"]: t for t in upstream}
    for t in FLEET_OVERLAY:
        by_name[t["name"]] = t

    tables = list(by_name.values())
    rendered = render(tables, version)
    OUT_PATH.write_text(rendered)
    print(
        f"wrote {OUT_PATH.relative_to(Path.cwd()) if OUT_PATH.is_relative_to(Path.cwd()) else OUT_PATH}: "
        f"{len(tables)} tables (upstream {len(upstream)}, overlay {len(FLEET_OVERLAY)})",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
